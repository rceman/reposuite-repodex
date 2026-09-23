//! Derived store — persistence + incremental/checkpoint driver + verify.
//!
//! Layout (`<derived>/`):
//!   manifest.json, checkpoint.json,
//!   sessions/<sid>.part.json      (persisted SessionPart per session)
//!   episodes/<inv>.json           (InvestigationEpisode per investigation)
//!   activity/paths.jsonl          (PathActivity records)
//!
//! Incremental model (§30-§34): sessions are append-only; the checkpoint
//! records each session's folded event count. On update only the tail events
//! beyond the checkpoint are folded into AgentActivity (add-only), the session
//! part is re-derived, and only the affected investigations are rebuilt.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::agent_event::AgentEventStore;
use crate::repository::digest;

use super::activity::ActivityIndex;
use super::episode::merge_episode;
use super::model::*;
use super::session::{derive_session, SessionPart};

/// Error for derived layer.
#[derive(Debug)]
pub enum DerivedError {
    Io { path: PathBuf, reason: String },
    Corrupt { reason: String },
    Stale { reason: String },
}

impl std::fmt::Display for DerivedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DerivedError::Io { path, reason } => write!(f, "io {}: {reason}", path.display()),
            DerivedError::Corrupt { reason } => write!(f, "corrupt derived: {reason}"),
            DerivedError::Stale { reason } => write!(f, "stale derived: {reason}"),
        }
    }
}
impl std::error::Error for DerivedError {}

fn safe(id: &str) -> String {
    id.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || "-_.$".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn write_json<T: serde::Serialize>(p: &Path, v: &T) -> Result<(), DerivedError> {
    std::fs::write(p, serde_json::to_string_pretty(v).unwrap_or_default()).map_err(|e| {
        DerivedError::Io {
            path: p.to_path_buf(),
            reason: e.to_string(),
        }
    })
}

/// In-memory derived store (loaded from disk or built fresh).
#[derive(Default)]
pub struct DerivedStore {
    pub checkpoint: DerivedCheckpoint,
    pub parts: BTreeMap<String, SessionPart>,
    pub episodes: BTreeMap<String, InvestigationEpisode>,
    pub activity: ActivityIndex,
}

impl Default for DerivedCheckpoint {
    fn default() -> Self {
        DerivedCheckpoint {
            derived_schema_version: DERIVED_SCHEMA_VERSION,
            event_store_digest: String::new(),
            session_processed: BTreeMap::new(),
        }
    }
}

impl DerivedStore {
    pub fn dir_session(d: &Path, sid: &str) -> PathBuf {
        d.join("sessions").join(format!("{}.part.json", safe(sid)))
    }
    pub fn dir_episode(d: &Path, inv: &str) -> PathBuf {
        d.join("episodes").join(format!("{}.json", safe(inv)))
    }

    /// Load persisted derived state (parts, episodes, activity, checkpoint).
    pub fn load(dir: &Path) -> Result<Self, DerivedError> {
        let mut ds = DerivedStore::default();
        let cp = dir.join("checkpoint.json");
        if cp.exists() {
            let t = std::fs::read_to_string(&cp).map_err(|e| DerivedError::Io {
                path: cp.clone(),
                reason: e.to_string(),
            })?;
            ds.checkpoint = serde_json::from_str(&t).map_err(|e| DerivedError::Corrupt {
                reason: format!("checkpoint: {e}"),
            })?;
        }
        // parts
        let sdir = dir.join("sessions");
        if sdir.is_dir() {
            for ent in std::fs::read_dir(&sdir).map_err(|e| DerivedError::Io {
                path: sdir.clone(),
                reason: e.to_string(),
            })? {
                let p = ent.map_err(|e| DerivedError::Io {
                    path: sdir.clone(),
                    reason: e.to_string(),
                })?;
                let pp = p.path();
                if pp.extension().and_then(|x| x.to_str()) == Some("json") {
                    let t = std::fs::read_to_string(&pp).map_err(|e| DerivedError::Io {
                        path: pp.clone(),
                        reason: e.to_string(),
                    })?;
                    let part: SessionPart =
                        serde_json::from_str(&t).map_err(|e| DerivedError::Corrupt {
                            reason: format!("{}: {e}", pp.display()),
                        })?;
                    ds.parts.insert(part.session_id.clone(), part);
                }
            }
        }
        // episodes
        let edir = dir.join("episodes");
        if edir.is_dir() {
            for ent in std::fs::read_dir(&edir).map_err(|e| DerivedError::Io {
                path: edir.clone(),
                reason: e.to_string(),
            })? {
                let pp = ent.map_err(|e| DerivedError::Io {
                    path: edir.clone(),
                    reason: e.to_string(),
                })?;
                let pp = pp.path();
                if pp.extension().and_then(|x| x.to_str()) == Some("json") {
                    let t = std::fs::read_to_string(&pp).map_err(|e| DerivedError::Io {
                        path: pp.clone(),
                        reason: e.to_string(),
                    })?;
                    let ep: InvestigationEpisode =
                        serde_json::from_str(&t).map_err(|e| DerivedError::Corrupt {
                            reason: format!("{}: {e}", pp.display()),
                        })?;
                    ds.episodes.insert(ep.investigation_id.clone(), ep);
                }
            }
        }
        // activity
        let ap = dir.join("activity").join("paths.jsonl");
        if ap.exists() {
            let t = std::fs::read_to_string(&ap).map_err(|e| DerivedError::Io {
                path: ap.clone(),
                reason: e.to_string(),
            })?;
            for line in t.lines() {
                if line.trim().is_empty() {
                    continue;
                }
                let rec: PathActivity =
                    serde_json::from_str(line).map_err(|e| DerivedError::Corrupt {
                        reason: format!("activity: {e}"),
                    })?;
                ds.activity
                    .paths
                    .insert((rec.repository_id.clone(), rec.path.clone()), rec);
            }
        }
        Ok(ds)
    }

    /// Persist all derived state atomically-ish.
    pub fn save(&self, dir: &Path) -> Result<(), DerivedError> {
        for d in ["sessions", "episodes", "activity"] {
            std::fs::create_dir_all(dir.join(d)).map_err(|e| DerivedError::Io {
                path: dir.to_path_buf(),
                reason: e.to_string(),
            })?;
        }
        for part in self.parts.values() {
            write_json(&Self::dir_session(dir, &part.session_id), part)?;
        }
        for ep in self.episodes.values() {
            write_json(&Self::dir_episode(dir, &ep.investigation_id), ep)?;
        }
        // activity jsonl
        let mut lines = String::new();
        for a in self.activity.paths.values() {
            lines.push_str(&serde_json::to_string(a).unwrap_or_default());
            lines.push('\n');
        }
        let ap = dir.join("activity").join("paths.jsonl");
        std::fs::write(&ap, lines).map_err(|e| DerivedError::Io {
            path: ap,
            reason: e.to_string(),
        })?;
        write_json(&dir.join("checkpoint.json"), &self.checkpoint)?;
        Ok(())
    }

    /// Persist manifest + digest.
    pub fn write_manifest(&self, dir: &Path) -> Result<DerivedManifest, DerivedError> {
        let mut bytes = 0u64;
        for part in self.parts.values() {
            bytes += serde_json::to_string(part).unwrap_or_default().len() as u64;
        }
        for ep in self.episodes.values() {
            bytes += serde_json::to_string(ep).unwrap_or_default().len() as u64;
        }
        for a in self.activity.paths.values() {
            bytes += serde_json::to_string(a).unwrap_or_default().len() as u64;
        }
        let m = DerivedManifest {
            schema: "reposuite.derived-agent-activity.v1".into(),
            derived_schema_version: DERIVED_SCHEMA_VERSION,
            manifest_version: DERIVED_MANIFEST_VERSION,
            investigations: self.episodes.len() as u64,
            sessions: self.parts.len() as u64,
            activity_paths: self.activity.paths.len() as u64,
            total_events_processed: self.checkpoint.session_processed.values().sum(),
            artifact_bytes: bytes,
            content_digest: digest::sha256_text(&format!(
                "{}:{}:{}",
                self.episodes.len(),
                self.activity.paths.len(),
                self.checkpoint.session_processed.values().sum::<u64>()
            )),
        };
        write_json(&dir.join("manifest.json"), &m)?;
        Ok(m)
    }
}

/// Result of a derive run.
#[derive(Debug, Default)]
pub struct DeriveReport {
    pub sessions_processed: u64,
    pub sessions_changed: u64,
    pub new_events_folded: u64,
    pub investigations_rebuilt: u64,
    pub episodes: u64,
    pub activity_paths: u64,
}

/// Build/update derived state from an AgentEventStore.
///
/// `full` = rebuild everything from raw events (§34). Otherwise incremental:
/// fold only sessions whose stored event count grew (§30). `known_paths` is the
/// canonical repo path set for final-answer mention extraction (§18).
pub fn derive(
    store: &AgentEventStore,
    out_dir: &Path,
    full: bool,
    known_paths: &BTreeSet<String>,
    repo_root: Option<&str>,
) -> Result<(DerivedStore, DeriveReport), DerivedError> {
    let mut ds = if full {
        DerivedStore::default()
    } else {
        DerivedStore::load(out_dir)?
    };
    if ds.checkpoint.derived_schema_version != DERIVED_SCHEMA_VERSION {
        return Err(DerivedError::Stale {
            reason: format!(
                "derived schema {} != {}",
                ds.checkpoint.derived_schema_version, DERIVED_SCHEMA_VERSION
            ),
        });
    }
    let mut rep = DeriveReport::default();
    let sids = store.sessions().map_err(|e| DerivedError::Corrupt {
        reason: e.to_string(),
    })?;
    let mut changed_invs: BTreeSet<String> = BTreeSet::new();
    let mut inv_sessions: BTreeMap<String, Vec<String>> = BTreeMap::new();
    // fold each session; only process sessions with new tail events (incremental)
    for sid in &sids {
        let events = store
            .session_events(sid)
            .map_err(|e| DerivedError::Corrupt {
                reason: e.to_string(),
            })?;
        let inv = events
            .first()
            .and_then(|e| e.investigation_id.clone())
            .unwrap_or_else(|| sid.clone());
        inv_sessions
            .entry(inv.clone())
            .or_default()
            .push(sid.clone());
        let processed = if full {
            0
        } else {
            *ds.checkpoint.session_processed.get(sid).unwrap_or(&0)
        };
        if (events.len() as u64) <= processed {
            rep.sessions_processed += 1;
            continue; // no new events
        }
        rep.sessions_changed += 1;
        rep.new_events_folded += events.len() as u64 - processed;
        // fold ONLY new tail events into activity (add-only)
        for e in &events[processed as usize..] {
            ds.activity.add_event(e, Some(&inv));
        }
        // re-derive the full session part (bounded) for episode rebuild
        let part = derive_session(&events);
        // fold final-answer path + evidence mentions into activity (idempotent)
        for (_, content) in &part.final_answers {
            let (pm, evm, _) =
                super::episode::extract_path_mentions(content, known_paths, repo_root);
            let repo = part.repository_id.clone().unwrap_or_default();
            for m in pm {
                ds.activity.add_mention(&repo, &m, sid);
            }
            for e in evm {
                ds.activity.add_evidence_mention(&repo, &e, sid);
            }
        }
        ds.parts.insert(sid.clone(), part);
        ds.checkpoint
            .session_processed
            .insert(sid.clone(), events.len() as u64);
        changed_invs.insert(inv);
        rep.sessions_processed += 1;
    }
    ds.checkpoint.event_store_digest = store.manifest_digest();
    // rebuild only affected investigations (§33); unchanged episodes stay as-is
    let rebuild: Vec<String> = if full {
        inv_sessions.keys().cloned().collect()
    } else {
        changed_invs.into_iter().collect()
    };
    for inv in &rebuild {
        let parts: Vec<SessionPart> = inv_sessions
            .get(inv)
            .map(|ss| ss.iter().filter_map(|s| ds.parts.get(s).cloned()).collect())
            .unwrap_or_default();
        if parts.is_empty() {
            continue;
        }
        let ep = merge_episode(inv, &parts, known_paths, repo_root);
        ds.episodes.insert(inv.clone(), ep);
        rep.investigations_rebuilt += 1;
    }
    rep.episodes = ds.episodes.len() as u64;
    rep.activity_paths = ds.activity.paths.len() as u64;
    Ok((ds, rep))
}

/// Verify derived-state integrity (§36): schema binding, checkpoint, repo
/// identity consistency, episode/session reference, digest. Returns anomalies.
pub fn verify(ds: &DerivedStore) -> Vec<String> {
    let mut bad = Vec::new();
    if ds.checkpoint.derived_schema_version != DERIVED_SCHEMA_VERSION {
        bad.push("derived_schema_version_mismatch".into());
    }
    for (sid, part) in &ds.parts {
        if part.session_id != *sid {
            bad.push(format!("part_id_mismatch:{sid}"));
        }
        if part.event_count as usize == 0 {
            bad.push(format!("empty_part:{sid}"));
        }
    }
    for (inv, ep) in &ds.episodes {
        if ep.investigation_id != *inv {
            bad.push(format!("episode_id_mismatch:{inv}"));
        }
        for sid in &ep.session_ids {
            if !ds.parts.contains_key(sid) {
                bad.push(format!("episode_missing_session:{inv}:{sid}"));
            }
        }
    }
    for ((repo, path), a) in &ds.activity.paths {
        if a.path != *path || a.repository_id != *repo {
            bad.push(format!("activity_key_mismatch:{path}"));
        }
    }
    bad
}
