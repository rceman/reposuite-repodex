//! Service configuration — `service.toml` in the RepoDex state dir (§7).
//!
//! Small, versioned, loopback-only by default. Values not present fall back to
//! conservative developer-workstation defaults.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub const SERVICE_CONFIG: &str = "service.toml";
pub const SERVICE_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ServiceConfig {
    /// Loopback host only (§8). Never 0.0.0.0 by default.
    pub host: String,
    /// 0 = OS-assigned ephemeral port, discovered after bind (§9).
    /// Non-zero = explicit fixed port; if unavailable, fail (§10).
    pub port: u16,
    pub memory: MemoryCfg,
    pub ingest: IngestCfg,
    pub derived: DerivedCfg,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MemoryCfg {
    /// Global hot-state budget (§35-§36). Conservative workstation default.
    pub max_mb: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct IngestCfg {
    /// Durable batch policy (§46): flush on first of these thresholds.
    pub flush_interval_ms: u64,
    pub flush_max_events: usize,
    pub flush_max_bytes: usize,
    /// Rotate the active segment when it exceeds this many bytes (§48-§50).
    pub segment_max_bytes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DerivedCfg {
    /// Checkpoint derived state after this interval or this many dirty updates
    /// (§52), whichever first.
    pub checkpoint_interval_s: u64,
    pub checkpoint_dirty_updates: u64,
}

impl Default for ServiceConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 0,
            memory: MemoryCfg::default(),
            ingest: IngestCfg::default(),
            derived: DerivedCfg::default(),
        }
    }
}
impl Default for MemoryCfg {
    fn default() -> Self {
        Self { max_mb: 2048 }
    }
}
impl Default for IngestCfg {
    fn default() -> Self {
        Self {
            flush_interval_ms: 100,
            flush_max_events: 256,
            flush_max_bytes: 128 * 1024,
            segment_max_bytes: 64 * 1024 * 1024,
        }
    }
}
impl Default for DerivedCfg {
    fn default() -> Self {
        Self {
            checkpoint_interval_s: 30,
            checkpoint_dirty_updates: 10_000,
        }
    }
}

impl ServiceConfig {
    pub fn load(state: &Path) -> Result<Self, String> {
        let p = state.join(SERVICE_CONFIG);
        if !p.exists() {
            return Ok(Self::default());
        }
        let t = fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?;
        toml::from_str(&t).map_err(|e| format!("{}: {e}", p.display()))
    }

    pub fn save(&self, state: &Path) -> Result<(), String> {
        fs::create_dir_all(state).map_err(|e| e.to_string())?;
        let p = state.join(SERVICE_CONFIG);
        let body = toml::to_string_pretty(self).map_err(|e| e.to_string())?;
        // Atomic write (§15-style): temp + rename.
        let tmp = p.with_extension("toml.tmp");
        fs::write(&tmp, body).map_err(|e| e.to_string())?;
        fs::rename(&tmp, &p).map_err(|e| e.to_string())
    }

    pub fn token_path(state: &Path) -> PathBuf {
        state.join("service.token")
    }
    pub fn runtime_path(state: &Path) -> PathBuf {
        state.join("service.runtime.json")
    }
    pub fn lock_path(state: &Path) -> PathBuf {
        state.join("service.lock")
    }
}
