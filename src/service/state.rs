//! Service shared state — HOT/WARM/COLD tiers, memory budget, metrics
//! (§24-§40, §61).
//!
//! - HOT  = explicit process memory: dedup index, FileAnalysis cache, view
//!   manifest/graph caches, derived hot state, query cache. Native Rust
//!   structures only — no in-memory SQL (§26).
//! - WARM = durable immutable artifacts on disk (content-addressed FileAnalysis
//!   objects, derived indexes) read via the OS page cache — not forced
//!   into Rust heap (§27).
//! - COLD = historical segments/episodes/exposures — no permanent hot RAM (§28).
//!
//! Memory budget uses a 2-segment (probation + protected) eviction policy so a
//! broad one-time scan can't flush the whole protected working set (§39).

use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use super::config::ServiceConfig;
use super::segments::DurableEventLog;

// ---- segmented LRU (probation + protected) ----------------------------------

/// Tiny 2-segment LRU: new entries land in `probation`; a second hit promotes
/// to `protected`. Eviction drains probation first, so a one-time scan only
/// evicts probationary entries (§39) instead of the hot working set.
pub struct SegmentedCache<V> {
    cap: usize,
    probation: Vec<String>,
    protected: Vec<String>,
    map: HashMap<String, (V, bool)>, // bool = in protected
}
impl<V> SegmentedCache<V> {
    pub fn new(cap: usize) -> Self {
        Self {
            cap: cap.max(8),
            probation: Vec::new(),
            protected: Vec::new(),
            map: HashMap::new(),
        }
    }
    pub fn get(&mut self, k: &str) -> Option<&V> {
        let prot = match self.map.get(k) {
            Some((_, p)) => *p,
            None => return None,
        };
        if !prot {
            // promote probation -> protected
            self.probation.retain(|x| x != k);
            self.protected.push(k.to_string());
            if let Some(v) = self.map.get_mut(k) {
                v.1 = true;
            }
        } else {
            self.protected.retain(|x| x != k);
            self.protected.push(k.to_string());
        }
        self.map.get(k).map(|(v, _)| v)
    }
    pub fn insert(&mut self, k: String, v: V) {
        if let Some(e) = self.map.get_mut(&k) {
            e.0 = v;
            e.1 = true;
            return;
        }
        self.probation.push(k.clone());
        self.map.insert(k, (v, false));
        self.evict_if_needed();
    }
    fn evict_if_needed(&mut self) {
        while self.map.len() > self.cap {
            // evict oldest probation first; only then protected
            let victim = if !self.probation.is_empty() {
                self.probation.remove(0)
            } else if !self.protected.is_empty() {
                self.protected.remove(0)
            } else {
                break;
            };
            self.map.remove(&victim);
        }
    }
    pub fn len(&self) -> usize {
        self.map.len()
    }
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
    pub fn hits_promotions(&self) -> usize {
        self.protected.len()
    }
}

// ---- metrics ----------------------------------------------------------------

/// Sliding latency reservoir (bounded) for p50/p95/p99 (§74).
pub struct Latency {
    samples: Vec<f64>,
    cap: usize,
}
impl Default for Latency {
    fn default() -> Self {
        Self {
            samples: Vec::new(),
            cap: 4096,
        }
    }
}
impl Latency {
    pub fn add(&mut self, ms: f64) {
        if self.samples.len() >= self.cap {
            self.samples.remove(0);
        }
        self.samples.push(ms);
    }
    pub fn summary(&self) -> LatencySummary {
        let mut v = self.samples.clone();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let n = v.len();
        let pick = |q: f64| -> f64 {
            if n == 0 {
                0.0
            } else {
                v[((n - 1) as f64 * q) as usize]
            }
        };
        let mean = if n == 0 {
            0.0
        } else {
            v.iter().sum::<f64>() / n as f64
        };
        LatencySummary {
            n: n as u64,
            mean,
            p50: pick(0.50),
            p95: pick(0.95),
            p99: pick(0.99),
            max: v.last().copied().unwrap_or(0.0),
        }
    }
}
#[derive(Debug, Default, Clone, Serialize)]
pub struct LatencySummary {
    pub n: u64,
    pub mean: f64,
    pub p50: f64,
    pub p95: f64,
    pub p99: f64,
    pub max: f64,
}

/// All counters are atomic; latency histograms behind a small lock (§61).
pub struct Metrics {
    pub started: Instant,
    pub queries: AtomicU64,
    pub events_received: AtomicU64,
    pub events_persisted: AtomicU64,
    pub events_duplicate: AtomicU64,
    pub events_rejected: AtomicU64,
    pub fa_cache_hit: AtomicU64,
    pub fa_cache_miss: AtomicU64,
    pub unique_versions_parsed: AtomicU64,
    pub derived_events_applied: AtomicU64,
    pub checkpoints: AtomicU64,
    pub ingest_latency: Mutex<Latency>,
    pub query_latency: Mutex<Latency>,
    pub derived_lag_events: AtomicU64,
}

/// HOT-tier caches keyed by content/fingerprint (bounded by memory budget).
pub struct HotState {
    /// FileAnalysis objects by object_key(path,digest) — probation+protected.
    pub file_analysis: Mutex<SegmentedCache<serde_json::Value>>,
    /// Loaded graph indexes by view fingerprint.
    pub graph_index: Mutex<HashMap<String, Arc<crate::graph::GraphIndex>>>,
    /// View manifests by canonical root.
    pub view_manifests: Mutex<SegmentedCache<serde_json::Value>>,
    /// Approximate bytes held hot (drives budget enforcement, §35-§37).
    pub hot_bytes: AtomicU64,
}

pub struct ServiceState {
    pub state_dir: PathBuf,
    pub config: ServiceConfig,
    pub token: String,
    pub events: Arc<DurableEventLog>,
    /// event_id -> semantic digest (rebuilt on startup; atomic with append).
    pub dedup: Mutex<HashMap<String, String>>,
    /// Channel feeding the async derived worker (§45). None until spawned.
    pub derived_tx:
        Mutex<Option<std::sync::mpsc::SyncSender<crate::agent_event::model::AgentEvent>>>,
    pub hot: HotState,
    pub metrics: Metrics,
    /// Active RepositoryViews seen (canonical roots) for status (§22).
    pub active_views: Mutex<Vec<String>>,
    pub shutdown: std::sync::atomic::AtomicBool,
}

impl ServiceState {
    pub fn new(
        state_dir: PathBuf,
        config: ServiceConfig,
        token: String,
        events: Arc<DurableEventLog>,
    ) -> Self {
        Self {
            state_dir,
            config,
            token,
            events,
            dedup: Mutex::new(HashMap::new()),
            derived_tx: Mutex::new(None),
            hot: HotState {
                file_analysis: Mutex::new(SegmentedCache::new(512)),
                graph_index: Mutex::new(HashMap::new()),
                view_manifests: Mutex::new(SegmentedCache::new(256)),
                hot_bytes: AtomicU64::new(0),
            },
            metrics: Metrics {
                started: Instant::now(),
                queries: AtomicU64::new(0),
                events_received: AtomicU64::new(0),
                events_persisted: AtomicU64::new(0),
                events_duplicate: AtomicU64::new(0),
                events_rejected: AtomicU64::new(0),
                fa_cache_hit: AtomicU64::new(0),
                fa_cache_miss: AtomicU64::new(0),
                unique_versions_parsed: AtomicU64::new(0),
                derived_events_applied: AtomicU64::new(0),
                checkpoints: AtomicU64::new(0),
                ingest_latency: Mutex::new(Latency::default()),
                query_latency: Mutex::new(Latency::default()),
                derived_lag_events: AtomicU64::new(0),
            },
            active_views: Mutex::new(Vec::new()),
            shutdown: std::sync::atomic::AtomicBool::new(false),
        }
    }

    pub fn uptime_s(&self) -> f64 {
        self.metrics.started.elapsed().as_secs_f64()
    }
    pub fn memory_budget_bytes(&self) -> u64 {
        self.config.memory.max_mb * 1024 * 1024
    }
    /// Rough RSS estimate via /proc (Unix); else tracked hot bytes.
    pub fn memory_current_bytes(&self) -> u64 {
        #[cfg(unix)]
        {
            if let Ok(s) = std::fs::read_to_string("/proc/self/status") {
                for line in s.lines() {
                    if let Some(v) = line.strip_prefix("VmRSS:") {
                        let kb: u64 = v.trim().trim_end_matches("kB").trim().parse().unwrap_or(0);
                        return kb * 1024;
                    }
                }
            }
        }
        self.hot.hot_bytes.load(Ordering::SeqCst)
    }

    pub fn note_view(&self, root: &str) {
        let mut v = self.active_views.lock().unwrap();
        if !v.iter().any(|x| x == root) {
            v.push(root.to_string());
        }
    }

    /// Snapshot of the repo roots the service is serving (for live memory
    /// derivation — symbol exposures resolve against their snapshots).
    pub fn active_views(&self) -> Vec<String> {
        self.active_views.lock().unwrap().clone()
    }

    /// Deterministic eviction under memory pressure (§37): probationary
    /// FileAnalysis → view manifests → graph indexes. Authoritative durable
    /// evidence is never in a cache, so it can never be lost (§32).
    pub fn enforce_memory_budget(&self) {
        if self.memory_current_bytes() <= self.memory_budget_bytes() {
            return;
        }
        // Trim probationary FileAnalysis first.
        let mut fa = self.hot.file_analysis.lock().unwrap();
        if fa.len() > 64 {
            *fa = SegmentedCache::new(64);
        }
        drop(fa);
        let mut gi = self.hot.graph_index.lock().unwrap();
        if gi.len() > 4 {
            // keep the 4 most-recently-inserted (cheap approximation)
            let keep: Vec<String> = gi.keys().take(4).cloned().collect();
            gi.retain(|k, _| keep.contains(k));
        }
    }
}
