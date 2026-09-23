//! AgentActivity index — accumulated per-(repository_id, path) usage derived
//! from `source_observed` + final-answer mentions (§23-§29). Observational
//! only; no usefulness/ranking. Add-only incremental: sessions are append-only
//! so folding new events is purely additive; distinct session/investigation
//! identity is kept as sets (idempotent re-fold, §26).

use std::collections::BTreeMap;

use crate::agent_event::AgentEvent;

use super::model::{PathActivity, ACTIVITY_WINDOW_DAYS};
use super::session::ts_day;

/// Key for the activity index.
fn key(repository_id: &str, path: &str) -> (String, String) {
    (repository_id.to_string(), path.to_string())
}

/// In-memory AgentActivity index (built once, then disk-persisted). Lookup is a
/// map get — no raw-event scan (§37).
#[derive(Default)]
pub struct ActivityIndex {
    pub paths: BTreeMap<(String, String), PathActivity>,
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
        let day = ts_day(&e.timestamp);
        let a = self
            .paths
            .entry(key(&repo, &path))
            .or_insert_with(|| PathActivity {
                repository_id: repo.clone(),
                path: path.clone(),
                first_observed_at: e.timestamp.clone(),
                last_observed_at: e.timestamp.clone(),
                ..Default::default()
            });
        a.observation_event_count_total += 1;
        match e
            .data
            .get("observation_kind")
            .and_then(|v| v.as_str())
            .unwrap_or("other")
        {
            "explicit_read" => a.explicit_read_count_total += 1,
            "search_snippet" => a.search_snippet_count_total += 1,
            "symbol_preview" => a.symbol_preview_count_total += 1,
            "diff" => a.diff_observation_count_total += 1,
            _ => a.other_observation_count_total += 1,
        }
        if let Some(b) = e.data.get("bytes").and_then(|v| v.as_u64()) {
            a.exposed_bytes_total += b;
        }
        if e.timestamp < a.first_observed_at {
            a.first_observed_at = e.timestamp.clone();
        }
        if e.timestamp > a.last_observed_at {
            a.last_observed_at = e.timestamp.clone();
        }
        if !a.session_ids.contains(&e.session_id) {
            a.session_ids.push(e.session_id.clone());
            a.session_ids.sort();
        }
        if let Some(inv) = investigation_id {
            if !a.investigation_ids.iter().any(|x| x == inv) {
                a.investigation_ids.push(inv.to_string());
                a.investigation_ids.sort();
            }
        }
        // daily bucket
        let is_read =
            e.data.get("observation_kind").and_then(|v| v.as_str()) == Some("explicit_read");
        let b = a.daily.entry(day).or_default();
        b.observations += 1;
        if is_read {
            b.explicit_reads += 1;
        }
        if let Some(inv) = investigation_id {
            if !b.investigation_ids.iter().any(|x| x == inv) {
                b.investigation_ids.push(inv.to_string());
            }
        }
        // prune buckets older than the retention window (§28)
        let max_day = a.daily.keys().next_back().copied().unwrap_or(day);
        let cutoff = max_day - ACTIVITY_WINDOW_DAYS;
        a.daily.retain(|d, _| *d >= cutoff);
    }

    /// Record a final-answer mention for a path in a session (§25). Idempotent
    /// per (session,path) via `mention_session_ids`.
    pub fn add_mention(&mut self, repo: &str, path: &str, session_id: &str) {
        let a = self
            .paths
            .entry(key(repo, path))
            .or_insert_with(|| PathActivity {
                repository_id: repo.to_string(),
                path: path.to_string(),
                first_observed_at: String::new(),
                last_observed_at: String::new(),
                ..Default::default()
            });
        if !a.mention_session_ids.iter().any(|s| s == session_id) {
            a.mention_session_ids.push(session_id.to_string());
            a.mention_session_ids.sort();
        }
    }

    /// Record a structured `EVIDENCE:` mention for a path in a session (§21).
    /// Idempotent per (session,path).
    pub fn add_evidence_mention(&mut self, repo: &str, path: &str, session_id: &str) {
        let a = self
            .paths
            .entry(key(repo, path))
            .or_insert_with(|| PathActivity {
                repository_id: repo.to_string(),
                path: path.to_string(),
                first_observed_at: String::new(),
                last_observed_at: String::new(),
                ..Default::default()
            });
        if !a
            .structured_evidence_session_ids
            .iter()
            .any(|s| s == session_id)
        {
            a.structured_evidence_session_ids
                .push(session_id.to_string());
            a.structured_evidence_session_ids.sort();
        }
    }

    /// All records in canonical (repo,path) order.
    pub fn paths_sorted(&self) -> Vec<&PathActivity> {
        self.paths.values().collect()
    }
}
