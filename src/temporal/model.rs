//! Git-derived temporal/activity model (TEMPORAL evidence family).
//!
//! This is repository-change metadata, NOT semantic code truth. A file being
//! `recent` or `hot` is an ACTIVITY observation about history; it never becomes
//! a FACT/CANDIDATE code relationship.
//!
//! ## V1 Git semantics (§12/§13)
//!
//! * change event = a non-merge commit's `--name-status` diff vs its first
//!   parent. Merge commits contribute no per-file change events (git suppresses
//!   the merge diff by default), so branch work and its merge are not
//!   double-counted. This is "development change events", one consistent unit.
//! * timestamps = committer time (`%ct`), i.e. when the change entered history.
//! * rename = `--no-renames`: a rename is `D old` + `A new`. History is NOT
//!   carried across a rename; the new path is first-seen at the rename commit
//!   (path-centric, explicit limitation).
//! * delete/re-create = same path continues one path history: `first_seen`
//!   stays the original add; `D`/re-`A` are ordinary change events.
//! * age/recency anchor to the indexed HEAD's committer timestamp so results
//!   are deterministic given (repo, HEAD, schema) — never wall-clock `now`.

use serde::{Deserialize, Serialize};

/// Temporal record schema version.
/// * `1` — initial path-centric file temporal index.
pub const TEMPORAL_SCHEMA_VERSION: u32 = 1;
/// Temporal artifact manifest format version.
pub const TEMPORAL_MANIFEST_VERSION: u32 = 1;
/// Temporal *policy* version — the Git->record semantics above. Bump when the
/// definition of a change event / rename / merge handling changes.
pub const TEMPORAL_POLICY_VERSION: u32 = 1;

/// Cap on the persisted most-recent change timestamps per file. Bounded memory
/// and disk; recent-window counts (30/90/365d) are exact while a file has at
/// most this many changes inside the window — a hotter file still classifies
/// correctly as very active.
pub const CHANGE_TAIL_CAP: usize = 128;

/// Per-file Git temporal aggregate. `path` is the canonical repository-relative
/// normalized path (never filesystem mtime).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileTemporal {
    /// Canonical repository-relative path.
    pub path: String,
    /// Oldest commit seen touching this path (path-centric identity).
    pub first_seen_commit: String,
    /// Committer unix seconds of `first_seen_commit`.
    pub first_seen_at: i64,
    /// Newest commit seen touching this path.
    pub last_changed_commit: String,
    /// Committer unix seconds of `last_changed_commit`.
    pub last_changed_at: i64,
    /// Number of change events (non-merge commits) touching this path.
    pub change_commit_count: u32,
    /// Most-recent change timestamps, ascending, capped at [`CHANGE_TAIL_CAP`].
    /// Drives 30/90/365-day window counts without rescanning history.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub change_ts_tail: Vec<i64>,
}

impl FileTemporal {
    /// Seconds between first and last observed change on the indexed history.
    pub fn tracked_age_seconds(&self) -> i64 {
        (self.last_changed_at - self.first_seen_at).max(0)
    }

    /// Seconds from first observed change to the indexed HEAD committer time.
    pub fn age_at_head(&self, head_ts: i64) -> i64 {
        (head_ts - self.first_seen_at).max(0)
    }

    /// Seconds from last change to the indexed HEAD committer time.
    pub fn recency_age_seconds(&self, head_ts: i64) -> i64 {
        (head_ts - self.last_changed_at).max(0)
    }

    /// Mean seconds between change events, when at least two events exist.
    pub fn mean_change_interval_seconds(&self) -> Option<i64> {
        if self.change_commit_count < 2 {
            return None;
        }
        Some(self.tracked_age_seconds() / i64::from(self.change_commit_count - 1))
    }

    /// Changes per year as fixed-point per-million (frequency*1e6), to avoid
    /// float nondeterminism. `None` when the tracked age is zero.
    pub fn change_frequency_per_year_e6(&self) -> Option<i64> {
        let age = self.tracked_age_seconds();
        if age <= 0 {
            return None;
        }
        Some(i64::from(self.change_commit_count) * 31_557_600 * 1_000_000 / age)
    }

    /// Change events whose committer time falls inside the last `days` before
    /// the indexed HEAD time. Exact while the file has <= cap events in window.
    pub fn changes_in_window(&self, head_ts: i64, days: i64) -> u32 {
        let cutoff = head_ts - days * 86_400;
        self.change_ts_tail
            .iter()
            .filter(|ts| **ts >= cutoff)
            .count() as u32
    }
}

/// The documented Git semantics binding for this artifact (§12/§13/§52/§53).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemporalGitSemantics {
    /// What one change event means.
    pub change_event: String,
    /// Timestamp source.
    pub timestamp: String,
    /// Merge handling.
    pub merge_policy: String,
    /// Rename handling.
    pub rename_policy: String,
    /// Delete/recreate handling.
    pub delete_recreate_policy: String,
}

impl Default for TemporalGitSemantics {
    fn default() -> Self {
        TemporalGitSemantics {
            change_event: "non-merge commit name-status diff vs first parent".to_string(),
            timestamp: "committer unix seconds (%ct)".to_string(),
            merge_policy: "merge commits contribute no per-file change events".to_string(),
            rename_policy: "--no-renames: D(old)+A(new); history not carried across rename"
                .to_string(),
            delete_recreate_policy: "one path history; delete and re-add are change events"
                .to_string(),
        }
    }
}

/// Temporal artifact manifest — binds records to repo identity + HEAD (§14).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalManifest {
    pub schema_version: u32,
    pub manifest_version: u32,
    pub policy_version: u32,
    /// sha256 of (root commit + remote url or canonical repo path).
    pub repository_identity: String,
    /// Root commit of the indexed history.
    pub root_commit: String,
    /// Indexed Git HEAD commit sha.
    pub indexed_head: String,
    /// Committer unix seconds of `indexed_head` (deterministic "as-of" anchor).
    pub indexed_head_ts: i64,
    pub semantics: TemporalGitSemantics,
    pub files_tracked: u64,
    pub commits_processed: u64,
    pub change_events: u64,
    /// sha256 over canonical record bytes.
    pub content_digest: String,
    pub artifact_bytes: u64,
    /// Build stats (diagnostic, not part of digest).
    pub build: TemporalBuildInfo,
}

/// Deterministic build/update stats for the manifest.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TemporalBuildInfo {
    pub mode: String,
    pub commits_processed: u64,
    pub change_events: u64,
    pub wall_ms: u64,
    pub peak_rss_kb: u64,
}
