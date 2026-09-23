//! Asynchronous derived-state worker (§45, §51-§53, §56, §77).
//!
//! Accepted AgentEvents are durable first, then a background worker updates the
//! HOT derived aggregates (`ActivityIndex`, session/episode state, memory
//! counters). Producers never block on this (§44).
//!
//! Checkpointing: derived state is aggregated in RAM and atomically checkpointed
//! every `checkpoint_interval_s` or `checkpoint_dirty_updates` — never per event
//! (§51-§52). On restart the service loads the latest checkpoint and replays the
//! committed tail after its `applied_events` watermark; `ActivityIndex::add_event`
//! is deterministic, so recovered incremental state == clean full replay (§77).

use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::agent_event::model::AgentEvent;
use crate::derived::activity::ActivityIndex;

use super::config::DerivedCfg;
use super::state::ServiceState;

pub const DERIVED_DIR: &str = "derived";
pub const CHECKPOINT_FILE: &str = "checkpoint.json";
pub const CHECKPOINT_SCHEMA: &str = "reposuite.repodex.derived-checkpoint.v1";

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DerivedCheckpoint {
    pub schema: String,
    /// Number of committed events already folded into `activity`.
    pub applied_events: u64,
    pub sessions: Vec<String>,
    /// Serialized ActivityIndex-derived totals (kept small; full index rebuilt
    /// by replay when needed for memory queries).
    pub total_tool_calls: u64,
    pub total_model_calls: u64,
    pub updated_at: String,
}

/// Live HOT derived state shared with status/query (§25).
pub struct DerivedHot {
    pub activity: Mutex<ActivityIndex>,
    pub sessions: Mutex<Vec<String>>,
    pub applied_events: AtomicU64,
    pub dirty_updates: AtomicU64,
    pub total_tool_calls: AtomicU64,
    pub total_model_calls: AtomicU64,
    pub last_checkpoint: Mutex<Instant>,
}

impl Default for DerivedHot {
    fn default() -> Self {
        Self {
            activity: Mutex::new(ActivityIndex::default()),
            sessions: Mutex::new(Vec::new()),
            applied_events: AtomicU64::new(0),
            dirty_updates: AtomicU64::new(0),
            total_tool_calls: AtomicU64::new(0),
            total_model_calls: AtomicU64::new(0),
            last_checkpoint: Mutex::new(Instant::now()),
        }
    }
}

/// Spawn the derived worker; returns the ingest channel producer.
pub fn spawn(state: Arc<ServiceState>, hot: Arc<DerivedHot>) -> SyncSender<AgentEvent> {
    let (tx, rx) = sync_channel::<AgentEvent>(10_000);
    thread::spawn(move || worker_loop(rx, state, hot));
    tx
}

fn worker_loop(rx: Receiver<AgentEvent>, state: Arc<ServiceState>, hot: Arc<DerivedHot>) {
    let cfg = state.config.derived.clone();
    let state_dir = state.state_dir.clone();
    let lag = &state.metrics.derived_lag_events;
    let applied_ctr = &state.metrics.derived_events_applied;
    let ck_ctr = &state.metrics.checkpoints;
    let mut pending: Vec<AgentEvent> = Vec::new();
    while !state.shutdown.load(Ordering::SeqCst) {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(e) => pending.push(e),
            Err(_) => {
                apply_batch(&pending, &hot, applied_ctr);
                lag.store(0, Ordering::SeqCst);
                pending.clear();
                maybe_checkpoint(&cfg, &state_dir, &hot, ck_ctr);
            }
        }
        if pending.len() >= 500 {
            apply_batch(&pending, &hot, applied_ctr);
            pending.clear();
            maybe_checkpoint(&cfg, &state_dir, &hot, ck_ctr);
        }
        lag.store(pending.len() as u64, Ordering::SeqCst);
    }
    // Drain remaining events on shutdown so nothing accepted is left un-derived.
    while let Ok(e) = rx.try_recv() {
        pending.push(e);
    }
    apply_batch(&pending, &hot, applied_ctr);
    let _ = checkpoint(&state_dir, &hot);
}

/// Fold a batch of events into the HOT derived state (incremental, §51).
fn apply_batch(events: &[AgentEvent], hot: &DerivedHot, applied_ctr: &AtomicU64) {
    if events.is_empty() {
        return;
    }
    let mut idx = hot.activity.lock().unwrap();
    let mut sessions = hot.sessions.lock().unwrap();
    for e in events {
        idx.add_event(e, e.investigation_id.as_deref());
        if !sessions.iter().any(|s| s == &e.session_id) {
            sessions.push(e.session_id.clone());
        }
        match e.event_type.as_str() {
            "tool_call_completed" => {
                hot.total_tool_calls.fetch_add(1, Ordering::SeqCst);
            }
            "model_call_completed" => {
                hot.total_model_calls.fetch_add(1, Ordering::SeqCst);
            }
            _ => {}
        }
    }
    drop(idx);
    drop(sessions);
    hot.applied_events
        .fetch_add(events.len() as u64, Ordering::SeqCst);
    hot.dirty_updates
        .fetch_add(events.len() as u64, Ordering::SeqCst);
    applied_ctr.fetch_add(events.len() as u64, Ordering::SeqCst);
}

fn maybe_checkpoint(cfg: &DerivedCfg, state_dir: &Path, hot: &DerivedHot, ck_ctr: &AtomicU64) {
    let dirty = hot.dirty_updates.load(Ordering::SeqCst);
    let elapsed = hot.last_checkpoint.lock().unwrap().elapsed();
    if dirty == 0 {
        return;
    }
    if dirty < cfg.checkpoint_dirty_updates
        && elapsed < Duration::from_secs(cfg.checkpoint_interval_s)
    {
        return;
    }
    if checkpoint(state_dir, hot).is_ok() {
        hot.dirty_updates.store(0, Ordering::SeqCst);
        *hot.last_checkpoint.lock().unwrap() = Instant::now();
        ck_ctr.fetch_add(1, Ordering::SeqCst);
    }
}

/// Atomically publish a derived checkpoint (tmp + rename) (§52).
pub fn checkpoint(state_dir: &Path, hot: &DerivedHot) -> Result<(), String> {
    let dir = state_dir.join(DERIVED_DIR);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let cp = DerivedCheckpoint {
        schema: CHECKPOINT_SCHEMA.into(),
        applied_events: hot.applied_events.load(Ordering::SeqCst),
        sessions: hot.sessions.lock().unwrap().clone(),
        total_tool_calls: hot.total_tool_calls.load(Ordering::SeqCst),
        total_model_calls: hot.total_model_calls.load(Ordering::SeqCst),
        updated_at: super::util::now_rfc3339(),
    };
    let p = dir.join(CHECKPOINT_FILE);
    let tmp = p.with_extension("json.tmp");
    fs::write(
        &tmp,
        serde_json::to_string_pretty(&cp).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    fs::rename(&tmp, &p).map_err(|e| e.to_string())
}

/// Load the latest checkpoint watermark (applied_events) if present (§53).
pub fn load_checkpoint(state_dir: &Path) -> Option<DerivedCheckpoint> {
    let t = fs::read_to_string(state_dir.join(DERIVED_DIR).join(CHECKPOINT_FILE)).ok()?;
    serde_json::from_str(&t).ok()
}
