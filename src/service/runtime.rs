//! Runtime discovery descriptor — `service.runtime.json` (§11-§12).
//!
//! Atomically published after a successful bind; removed on graceful stop.
//! `status`/`start` validate the PID is alive rather than trusting the file —
//! a crashed instance leaves a stale descriptor that must be detected (§12).
//! The token is NEVER stored here (§16).

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub const RUNTIME_FILE: &str = "service.runtime.json";
pub const RUNTIME_SCHEMA: &str = "reposuite.repodex.service.runtime.v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeDescriptor {
    pub schema: String,
    pub pid: u32,
    pub host: String,
    pub port: u16,
    pub instance_id: String,
    pub started_at: String,
    pub repodex_version: String,
    pub protocol_version: u32,
}

impl RuntimeDescriptor {
    pub fn new(host: &str, port: u16, instance_id: &str) -> Self {
        Self {
            schema: RUNTIME_SCHEMA.to_string(),
            pid: std::process::id(),
            host: host.to_string(),
            port,
            instance_id: instance_id.to_string(),
            started_at: super::util::now_rfc3339(),
            repodex_version: env!("CARGO_PKG_VERSION").to_string(),
            protocol_version: 1,
        }
    }
}

/// Atomically publish the descriptor (temp + rename) (§11).
pub fn publish(state: &Path, d: &RuntimeDescriptor) -> Result<(), String> {
    fs::create_dir_all(state).map_err(|e| e.to_string())?;
    let p = state.join(RUNTIME_FILE);
    let tmp = p.with_extension("json.tmp");
    fs::write(
        &tmp,
        serde_json::to_string_pretty(d).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    fs::rename(&tmp, &p).map_err(|e| e.to_string())
}

/// Remove the descriptor on graceful stop (§12).
pub fn remove(state: &Path) {
    let _ = fs::remove_file(state.join(RUNTIME_FILE));
    let _ = fs::remove_file(state.join("service.runtime.json.tmp"));
}

/// Read + validate the descriptor: returns `Some` only if the file parses AND
/// its PID is currently alive (stale/crashed detection, §12).
pub fn live(state: &Path) -> Option<RuntimeDescriptor> {
    let t = fs::read_to_string(state.join(RUNTIME_FILE)).ok()?;
    let d: RuntimeDescriptor = serde_json::from_str(&t).ok()?;
    if pid_alive(d.pid) {
        Some(d)
    } else {
        None
    }
}

/// Raw descriptor without liveness check (for diagnostics).
pub fn read(state: &Path) -> Option<RuntimeDescriptor> {
    let t = fs::read_to_string(state.join(RUNTIME_FILE)).ok()?;
    serde_json::from_str(&t).ok()
}

/// Is a process with this PID alive? (cross-platform best-effort)
pub fn pid_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        // kill(pid, 0): exists-check without signaling.
        unsafe {
            libc_kill(pid as i32, 0) == 0
                || std::io::Error::last_os_error().raw_os_error() == Some(1)
        }
    }
    #[cfg(not(unix))]
    {
        pid > 0 // best-effort: assume alive; refined by TCP probe in status
    }
}

#[cfg(unix)]
unsafe fn libc_kill(pid: i32, sig: i32) -> i32 {
    extern "C" {
        fn kill(pid: i32, sig: i32) -> i32;
    }
    kill(pid, sig)
}

pub fn lock_path(state: &Path) -> PathBuf {
    state.join("service.lock")
}
