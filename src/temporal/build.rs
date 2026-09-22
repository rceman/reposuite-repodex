//! Temporal index build + incremental update driver.
//!
//! Full build streams the whole history once; update processes only
//! `OLD_HEAD..NEW_HEAD` when `OLD_HEAD` is an ancestor — otherwise it refuses
//! with `history_diverged` (§15-17). Persistence is atomic via staging.

use std::path::Path;
use std::time::Instant;

use crate::repository::digest;

use super::artifact;
use super::collect::{self, CollectResult};
use super::model::*;
use super::TemporalError;

/// Repo identity = sha256(root_commit + remote_url_or_path) (§14).
pub fn repository_identity(repo: &Path) -> Result<String, TemporalError> {
    let root = collect::root_commit(repo)?;
    let url = collect::remote_url(repo);
    let basis = if url.is_empty() {
        repo.canonicalize()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| repo.to_string_lossy().to_string())
    } else {
        url
    };
    Ok(digest::sha256_text(&format!("{root}\n{basis}")))
}

/// Build/update outcome.
#[derive(Debug)]
pub struct TemporalOutcome {
    pub manifest: TemporalManifest,
    pub stats: TemporalBuildInfo,
}

/// Peak resident set size (VmHWM) in KB, read from /proc — no dependency.
fn peak_rss_kb() -> u64 {
    let Ok(text) = std::fs::read_to_string("/proc/self/status") else {
        return 0;
    };
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("VmHWM:") {
            return rest
                .trim()
                .trim_end_matches("kB")
                .trim()
                .parse()
                .unwrap_or(0);
        }
    }
    0
}

fn finalize(
    repo: &Path,
    output: &Path,
    acc: CollectResult,
    head: &str,
    head_ts: i64,
    mode: &str,
    started: Instant,
) -> Result<TemporalOutcome, TemporalError> {
    let staging = artifact::prepare_staging(output)?;
    let files: Vec<FileTemporal> = acc.files.into_values().collect();
    let bytes = artifact::write_records(&staging, &files)?;
    let content_digest = artifact::records_digest(&staging)?;
    let stats = TemporalBuildInfo {
        mode: mode.to_string(),
        commits_processed: acc.commits_processed,
        change_events: acc.change_events,
        wall_ms: started.elapsed().as_millis() as u64,
        peak_rss_kb: peak_rss_kb(),
    };
    let manifest = TemporalManifest {
        schema_version: TEMPORAL_SCHEMA_VERSION,
        manifest_version: TEMPORAL_MANIFEST_VERSION,
        policy_version: TEMPORAL_POLICY_VERSION,
        repository_identity: repository_identity(repo)?,
        root_commit: collect::root_commit(repo)?,
        indexed_head: head.to_string(),
        indexed_head_ts: head_ts,
        semantics: TemporalGitSemantics::default(),
        files_tracked: files.len() as u64,
        commits_processed: acc.commits_processed,
        change_events: acc.change_events,
        content_digest,
        artifact_bytes: bytes,
        build: stats.clone(),
    };
    artifact::write_manifest(&staging, &manifest)?;
    artifact::publish(&staging, output)?;
    Ok(TemporalOutcome { manifest, stats })
}

/// Full build: stream entire history once into a fresh temporal index.
pub fn build_temporal(repo: &Path, output: &Path) -> Result<TemporalOutcome, TemporalError> {
    let started = Instant::now();
    let head = collect::head_commit(repo)?;
    let head_ts = collect::commit_ts(repo, &head)?;
    let acc = collect::collect(repo, None)?;
    finalize(repo, output, acc, &head, head_ts, "full", started)
}

/// Incremental update: apply only `OLD..NEW` onto the prior index.
///
/// Fails with `history_diverged` when the recorded head is not an ancestor of
/// the current HEAD (rewritten/rebased history) — never a silent wrong update.
pub fn update_temporal(
    repo: &Path,
    previous: &Path,
    output: &Path,
) -> Result<TemporalOutcome, TemporalError> {
    let started = Instant::now();
    let prev_manifest = artifact::verify(previous)?;
    // Repository binding must match, or we'd blend histories.
    if prev_manifest.repository_identity != repository_identity(repo)? {
        return Err(TemporalError::UpstreamMismatch {
            reason: "repository identity mismatch".to_string(),
        });
    }
    let old_head = prev_manifest.indexed_head.clone();
    let new_head = collect::head_commit(repo)?;
    if old_head == new_head {
        // Nothing new: republish an equivalent index bound to the same head.
        let files = artifact::read_files(previous)?;
        let acc = CollectResult {
            files: files.into_iter().map(|f| (f.path.clone(), f)).collect(),
            commits_processed: 0,
            change_events: 0,
        };
        return finalize(
            repo,
            output,
            acc,
            &new_head,
            prev_manifest.indexed_head_ts,
            "noop",
            started,
        );
    }
    if !collect::is_ancestor(repo, &old_head, &new_head)? {
        return Err(TemporalError::HistoryDiverged { old_head, new_head });
    }
    let new_ts = collect::commit_ts(repo, &new_head)?;
    // Load prior records, then stream only the new range.
    let mut acc = CollectResult {
        files: artifact::read_files(previous)?
            .into_iter()
            .map(|f| (f.path.clone(), f))
            .collect(),
        commits_processed: 0,
        change_events: 0,
    };
    let range = format!("{old_head}..{new_head}");
    collect::collect_into(repo, Some(&range), &mut acc, false)?;
    finalize(repo, output, acc, &new_head, new_ts, "incremental", started)
}
