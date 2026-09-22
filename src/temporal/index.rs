//! Query-time temporal index: a compact in-memory map over the persisted
//! artifact. Lookups never touch Git (§3/§28).

use std::collections::BTreeMap;
use std::path::Path;

use super::artifact;
use super::model::{FileTemporal, TemporalManifest};
use super::TemporalError;

/// A loaded temporal index: manifest + per-file records.
pub struct TemporalIndex {
    pub manifest: TemporalManifest,
    files: BTreeMap<String, FileTemporal>,
}

impl TemporalIndex {
    /// Load and verify an artifact directory.
    pub fn load(dir: &Path) -> Result<Self, TemporalError> {
        let manifest = artifact::verify(dir)?;
        let files = artifact::read_files(dir)?;
        let map = files.into_iter().map(|f| (f.path.clone(), f)).collect();
        Ok(TemporalIndex {
            manifest,
            files: map,
        })
    }

    /// Number of tracked files.
    pub fn len(&self) -> usize {
        self.files.len()
    }

    /// True when empty.
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// Look up one canonical repository-relative path. No Git is run.
    pub fn file(&self, path: &str) -> Option<&FileTemporal> {
        self.files.get(path)
    }

    /// Iterate records in canonical path order.
    pub fn files(&self) -> impl Iterator<Item = &FileTemporal> {
        self.files.values()
    }

    /// A flat enriched view of one file for inspection/JSON output.
    pub fn describe(&self, path: &str) -> Option<serde_json::Value> {
        let f = self.file(path)?;
        let h = self.manifest.indexed_head_ts;
        Some(serde_json::json!({
            "path": f.path,
            "first_seen_commit": f.first_seen_commit,
            "first_seen_at": f.first_seen_at,
            "last_changed_commit": f.last_changed_commit,
            "last_changed_at": f.last_changed_at,
            "change_commit_count": f.change_commit_count,
            "tracked_age_seconds": f.tracked_age_seconds(),
            "age_seconds_at_head": f.age_at_head(h),
            "recency_age_seconds": f.recency_age_seconds(h),
            "mean_change_interval_seconds": f.mean_change_interval_seconds(),
            "change_frequency_per_year_e6": f.change_frequency_per_year_e6(),
            "changes_30d": f.changes_in_window(h, 30),
            "changes_90d": f.changes_in_window(h, 90),
            "changes_365d": f.changes_in_window(h, 365),
        }))
    }
}
