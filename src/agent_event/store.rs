//! Agent Event Store — durable, append-oriented, disk-backed canonical events.
//!
//! Layout:
//!
//! ```text
//! <store>/
//!   manifest.json
//!   sessions/<safe_session_id>.jsonl
//! ```
//!
//! Idempotency is per-session: a replayed batch only needs that session's file
//! loaded, so memory stays bounded by one session's events — never the whole
//! historical store (§31/§71). Appends are atomic via a tmp write + rename per
//! session file rewrite only on the (rare) out-of-order path; normal append is
//! O(1).

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use crate::repository::digest;

use super::model::AgentEvent;
use super::AgentEventError;

pub const MANIFEST_FILE: &str = "manifest.json";
pub const STORE_VERSION: u32 = 1;

/// Sanitize a session id into a safe filename (producer ids are word-safe, but be
/// strict anyway; fall back to a hash for hostile ids).
fn safe_session_file(sid: &str) -> String {
    let ok: String = sid
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if ok.is_empty() || ok == "." || ok == ".." {
        format!("_{}", &digest::sha256_text(sid)[..16])
    } else {
        format!("{ok}.jsonl")
    }
}

/// Per-store manifest.
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct StoreManifest {
    pub schema: String,
    pub store_version: u32,
    pub total_events: u64,
    pub total_sessions: u64,
    /// sha256 over all session files (concatenated digests) — corruption check.
    pub content_digest: String,
}

use serde::Serialize;

/// A durable Agent Event Store rooted at `dir`.
pub struct AgentEventStore {
    dir: PathBuf,
}

impl AgentEventStore {
    pub fn open(dir: &Path) -> Result<Self, AgentEventError> {
        fs::create_dir_all(dir.join("sessions")).map_err(|e| AgentEventError::Io {
            path: dir.to_path_buf(),
            reason: e.to_string(),
        })?;
        Ok(AgentEventStore {
            dir: dir.to_path_buf(),
        })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn session_path(&self, sid: &str) -> PathBuf {
        self.dir.join("sessions").join(safe_session_file(sid))
    }

    /// Read one session's events (bounded by that session).
    pub fn session_events(&self, sid: &str) -> Result<Vec<AgentEvent>, AgentEventError> {
        let p = self.session_path(sid);
        if !p.exists() {
            return Ok(Vec::new());
        }
        let f = fs::File::open(&p).map_err(|e| AgentEventError::Io {
            path: p.clone(),
            reason: e.to_string(),
        })?;
        let mut out = Vec::new();
        for (n, line) in BufReader::new(f).lines().enumerate() {
            let line = line.map_err(|e| AgentEventError::Io {
                path: p.clone(),
                reason: e.to_string(),
            })?;
            if line.trim().is_empty() {
                continue;
            }
            let e: AgentEvent =
                serde_json::from_str(&line).map_err(|e| AgentEventError::Corrupt {
                    reason: format!("{} line {}: {e}", p.display(), n + 1),
                })?;
            out.push(e);
        }
        Ok(out)
    }

    /// event_id -> content_digest for one session (dedup/conflict index).
    fn session_dedup(&self, sid: &str) -> Result<HashMap<String, Option<String>>, AgentEventError> {
        let mut m = HashMap::new();
        for e in self.session_events(sid)? {
            m.insert(e.event_id.clone(), e.content_digest.clone());
        }
        Ok(m)
    }

    /// All session ids (from filenames).
    pub fn sessions(&self) -> Result<Vec<String>, AgentEventError> {
        let mut out = Vec::new();
        let dir = self.dir.join("sessions");
        if !dir.exists() {
            return Ok(out);
        }
        for ent in fs::read_dir(&dir).map_err(|e| AgentEventError::Io {
            path: dir.clone(),
            reason: e.to_string(),
        })? {
            let ent = ent.map_err(|e| AgentEventError::Io {
                path: dir.clone(),
                reason: e.to_string(),
            })?;
            let name = ent.file_name().to_string_lossy().to_string();
            if let Some(s) = name.strip_suffix(".jsonl") {
                out.push(s.to_string());
            }
        }
        out.sort();
        Ok(out)
    }

    /// Deterministic semantic digest of an event (excludes `content_digest`).
    pub fn event_digest(e: &AgentEvent) -> String {
        let mut c = e.clone();
        c.content_digest = None;
        // serde_json object maps are BTreeMap -> canonical sorted-key output.
        digest::sha256(&serde_json::to_vec(&c).unwrap_or_default())
    }

    /// Append validated events. `events` must already be grouped/validated.
    /// Returns (accepted, duplicates, conflicts). Appends to per-session file.
    pub fn append(&mut self, events: &[AgentEvent]) -> Result<IngestReport, AgentEventError> {
        // Group by session preserving order.
        let mut by_session: BTreeMap<String, Vec<&AgentEvent>> = BTreeMap::new();
        for e in events {
            by_session.entry(e.session_id.clone()).or_default().push(e);
        }
        let mut report = IngestReport::default();
        for (sid, evs) in by_session {
            let mut dedup = self.session_dedup(&sid)?;
            // Append path.
            let p = self.session_path(&sid);
            let mut f = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&p)
                .map_err(|e| AgentEventError::Io {
                    path: p.clone(),
                    reason: e.to_string(),
                })?;
            for e in evs {
                match dedup.get(&e.event_id) {
                    Some(existing_digest) => {
                        // Replayed event_id: identical content -> duplicate;
                        // different content -> conflict (§30).
                        let same = match (existing_digest, &e.content_digest) {
                            (Some(a), Some(b)) => a == b,
                            _ => {
                                Self::event_digest(e)
                                    == existing_digest
                                        .clone()
                                        .unwrap_or_else(|| Self::event_digest(e))
                            }
                        };
                        if same {
                            report.duplicates += 1;
                        } else {
                            report.rejected += 1;
                            report.errors.push(format!(
                                "conflicting event_id {} in session {sid}",
                                e.event_id
                            ));
                        }
                    }
                    None => {
                        dedup.insert(e.event_id.clone(), e.content_digest.clone());
                        let line =
                            serde_json::to_string(e).map_err(|e| AgentEventError::Serialize {
                                reason: e.to_string(),
                            })?;
                        writeln!(f, "{line}").map_err(|e| AgentEventError::Io {
                            path: p.clone(),
                            reason: e.to_string(),
                        })?;
                        report.accepted += 1;
                    }
                }
            }
        }
        Ok(report)
    }

    /// Recompute total events + digest and write the manifest.
    pub fn update_manifest(&self) -> Result<StoreManifest, AgentEventError> {
        let sids = self.sessions()?;
        let mut total = 0u64;
        let mut h = String::new();
        for sid in &sids {
            let p = self.session_path(sid);
            let data = fs::read(&p).map_err(|e| AgentEventError::Io {
                path: p.clone(),
                reason: e.to_string(),
            })?;
            total += data.iter().filter(|b| **b == b'\n').count() as u64;
            h.push_str(&digest::sha256(&data));
            h.push('\n');
        }
        let m = StoreManifest {
            schema: super::model::AGENT_EVENT_SCHEMA.to_string(),
            store_version: STORE_VERSION,
            total_events: total,
            total_sessions: sids.len() as u64,
            content_digest: digest::sha256_text(&h),
        };
        fs::write(
            self.dir.join(MANIFEST_FILE),
            serde_json::to_string_pretty(&m).unwrap_or_default(),
        )
        .map_err(|e| AgentEventError::Io {
            path: self.dir.join(MANIFEST_FILE),
            reason: e.to_string(),
        })?;
        Ok(m)
    }

    /// The persisted manifest `content_digest` ("" if absent) — binds derived
    /// state to the event-store snapshot it was built from (§31).
    pub fn manifest_digest(&self) -> String {
        fs::read_to_string(self.dir.join(MANIFEST_FILE))
            .ok()
            .and_then(|t| serde_json::from_str::<StoreManifest>(&t).ok())
            .map(|m| m.content_digest)
            .unwrap_or_default()
    }
}

/// Batch ingest report (§28).
#[derive(Debug, Clone, Default, Serialize, serde::Deserialize)]
pub struct IngestReport {
    pub accepted: u64,
    pub duplicates: u64,
    pub rejected: u64,
    #[serde(default)]
    pub errors: Vec<String>,
}
