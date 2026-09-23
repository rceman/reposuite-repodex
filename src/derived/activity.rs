//! AgentActivity index — accumulated per-(repository_id, path) usage derived
//! from `source_observed` + final-answer mentions (§23-§29). Observational
//! only; no usefulness/ranking. Add-only incremental: sessions are append-only
//! so folding new events is purely additive; distinct session/investigation
//! identity is exact via a disk-backed contribution index (§12-§13).
//!
//! Hot `PathActivity` records hold COUNTS + bounded daily buckets only — never
//! an unbounded list of every session/investigation id (§12). The membership
//! sets live in `contrib` (§13), persisted separately and read only during
//! build/incremental, not during a lookup.

use std::collections::BTreeMap;

use crate::agent_event::AgentEvent;

use super::model::{ContribMembership, PathActivity, ACTIVITY_WINDOW_DAYS};
use super::session::ts_day;

/// Key for the activity index.
fn key(repository_id: &str, path: &str) -> (String, String) {
    (repository_id.to_string(), path.to_string())
}

/// AgentActivity index. `paths` = hot records (counts, bounded). `contrib` =
/// exact distinct-identity membership for incremental/counting correctness,
/// persisted separately so a hot record never grows with session count.
#[derive(Default)]
pub struct ActivityIndex {
    pub paths: BTreeMap<(String, String), PathActivity>,
    /// Exact distinct membership per (repo,path); NOT loaded for lookup.
    pub contrib: BTreeMap<(String, String), ContribMembership>,
}

impl ActivityIndex {
    /// Fold one canonical event into the index. Only `source_observed` events
    /// contribute. `now`-independent (timestamps come from the event).
    pub fn add_event(&mut self, e: &AgentEvent, investigation_id: Option<&str>) {
        if e.event_type != "source_observed" {
            return;
        }
        let repo = e.repository_id.clone().unwrap_or_else(|| "".into());
        let path = match e.data.get("path").and_then(|v| v.as_str()) {
            Some(p) if !p.is_empty() => p.to_string(),
            _ => return,
        };
        let k = key(&repo, &path);
        let day = ts_day(&e.timestamp);
        let kind = e
            .data
            .get("observation_kind")
            .and_then(|v| v.as_str())
            .unwrap_or("other");
        let bytes = e.data.get("bytes").and_then(|v| v.as_u64()).unwrap_or(0);
        // distinct membership (exact) in the contribution index
        let cm = self.contrib.entry(k.clone()).or_default();
        if !cm.session_ids.contains(&e.session_id) {
            cm.session_ids.push(e.session_id.clone());
        }
        if let Some(inv) = investigation_id {
            if !cm.investigation_ids.iter().any(|x| x.as_str() == inv) {
                cm.investigation_ids.push(inv.to_string());
            }
        }
        let sess_n = cm.session_ids.len() as u64;
        let inv_n = cm.investigation_ids.len() as u64;
        // hot record: counters + counts + bounded daily buckets
        let a = self.paths.entry(k).or_insert_with(|| PathActivity {
            repository_id: repo.clone(),
            path: path.clone(),
            first_observed_at: e.timestamp.clone(),
            last_observed_at: e.timestamp.clone(),
            ..Default::default()
        });
        a.observation_event_count_total += 1;
        match kind {
            "explicit_read" => a.explicit_read_count_total += 1,
            "search_snippet" => a.search_snippet_count_total += 1,
            "symbol_preview" => a.symbol_preview_count_total += 1,
            "diff" => a.diff_observation_count_total += 1,
            _ => a.other_observation_count_total += 1,
        }
        a.exposed_bytes_total += bytes;
        a.sessions_observed_count = sess_n;
        a.investigations_observed_count = inv_n;
        if e.timestamp < a.first_observed_at {
            a.first_observed_at = e.timestamp.clone();
        }
        if e.timestamp > a.last_observed_at {
            a.last_observed_at = e.timestamp.clone();
        }
        // daily bucket (per-day investigation ids stay bounded by that day)
        let b = a.daily.entry(day).or_default();
        b.observations += 1;
        if kind == "explicit_read" {
            b.explicit_reads += 1;
        }
        if let Some(inv) = investigation_id {
            if !b.investigation_ids.iter().any(|x| x.as_str() == inv) {
                b.investigation_ids.push(inv.to_string());
            }
        }
        let max_day = a.daily.keys().next_back().copied().unwrap_or(day);
        let cutoff = max_day - ACTIVITY_WINDOW_DAYS;
        a.daily.retain(|d, _| *d >= cutoff);
    }

    /// Record a final-answer mention for a path in a session (§25). Idempotent
    /// per (session,path) via the contribution set.
    pub fn add_mention(&mut self, repo: &str, path: &str, session_id: &str) {
        let k = key(repo, path);
        let cm = self.contrib.entry(k.clone()).or_default();
        if !cm
            .mention_session_ids
            .iter()
            .any(|s| s.as_str() == session_id)
        {
            cm.mention_session_ids.push(session_id.to_string());
            let n = cm.mention_session_ids.len() as u64;
            let a = self.paths.entry(k).or_insert_with(|| PathActivity {
                repository_id: repo.to_string(),
                path: path.to_string(),
                ..Default::default()
            });
            a.final_answer_mention_count = n;
        }
    }

    /// Record a structured `EVIDENCE:` mention (§21). Idempotent per session.
    pub fn add_evidence_mention(&mut self, repo: &str, path: &str, session_id: &str) {
        let k = key(repo, path);
        let cm = self.contrib.entry(k.clone()).or_default();
        if !cm
            .evidence_session_ids
            .iter()
            .any(|s| s.as_str() == session_id)
        {
            cm.evidence_session_ids.push(session_id.to_string());
            let a = self.paths.entry(k).or_insert_with(|| PathActivity {
                repository_id: repo.to_string(),
                path: path.to_string(),
                ..Default::default()
            });
            a.structured_evidence_mention_count = cm.evidence_session_ids.len() as u64;
        }
    }

    /// All records in canonical (repo,path) order.
    pub fn paths_sorted(&self) -> Vec<&PathActivity> {
        self.paths.values().collect()
    }
}
