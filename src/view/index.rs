//! View-aware index service (§26-§28).
//!
//! A RepositoryView's content is fingerprinted by its effective manifest
//! (path → content digest). The derived index for that content is stored under
//! `indexes/{fingerprint}/` and reused whenever the view's content is identical
//! — so N worktrees with identical source share ONE derived index and produce
//! ~zero incremental disk (§75-§76). `FileAnalysis` objects are additionally
//! content-addressed in a shared `objects/` store keyed by `object_key(path,
//! digest)`, so identical files across *different* contents are never reparsed.

use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::model::LanguageId;
use crate::parser::Analyzer;
use crate::repository::artifact;
use crate::repository::build;
use crate::repository::fingerprint::{AnalyzerFingerprint, SnapshotConfig};
use crate::repository::manifest::RepositoryManifest;

use super::manifest::{self, ViewManifest};
use super::model::RepositoryView;
use super::registry::state_dir;

/// Result of ensuring a view's derived index exists.
#[derive(Debug, Clone, Serialize)]
pub struct EnsureOutcome {
    /// Graph index dir to query.
    pub graph_dir: PathBuf,
    /// View content fingerprint (index key).
    pub fingerprint: String,
    /// True when the whole derived index already existed for this content.
    pub index_reused: bool,
    /// FileAnalysis objects reused from the shared store (no reparse).
    pub analyses_reused: u64,
    /// FileAnalysis objects newly parsed.
    pub analyses_parsed: u64,
    /// Files in the view manifest.
    pub files: u64,
    /// Wall time to ensure the index (manifest compute + any build), ms.
    pub ensure_ms: f64,
}

fn objects_dir(state: &Path) -> PathBuf {
    state.join("objects")
}
fn indexes_dir(state: &Path) -> PathBuf {
    state.join("indexes")
}

/// Hardlink `src` to `dst` (same filesystem). Falls back to copy.
fn link_or_copy(src: &Path, dst: &Path) -> std::io::Result<()> {
    if let Some(d) = dst.parent() {
        fs::create_dir_all(d)?;
    }
    if fs::hard_link(src, dst).is_ok() {
        return Ok(());
    }
    fs::copy(src, dst).map(|_| ())
}

/// Build the view's snapshot into `snap_dir`, reusing shared FileAnalysis
/// objects by `object_key(path, digest)` (§27). Returns the manifest +
/// (reused, parsed) counts.
fn build_view_snapshot(
    analyzer: &Analyzer,
    view: &RepositoryView,
    manifest: &ViewManifest,
    snap_dir: &Path,
    state: &Path,
    reused: &mut u64,
    parsed: &mut u64,
) -> Result<RepositoryManifest, String> {
    let fingerprint = AnalyzerFingerprint::current(analyzer.registry());
    let config = SnapshotConfig::new(analyzer.config().max_file_size, true);
    let mut m = RepositoryManifest::new(&fingerprint, config);
    let obj_root = objects_dir(state);
    fs::create_dir_all(obj_root.join("files")).map_err(|e| e.to_string())?;
    let snap_files = snap_dir.join("files");
    fs::create_dir_all(&snap_files).map_err(|e| e.to_string())?;

    for vf in &manifest.files {
        let objkey = artifact::object_key(&vf.relative_path, &vf.content_digest);
        let obj_path = obj_root.join("files").join(format!("{objkey}.json"));
        let snap_obj = snap_files.join(format!("{objkey}.json"));
        let indexed = if obj_path.exists() {
            // Reuse: load the content-addressed analysis for the manifest record,
            // hardlink it into this snapshot (zero-copy dedup).
            let analysis = read_analysis(&obj_path)?;
            let rec = build::index_file(&analysis, &vf.content_digest, vf.source_bytes, false);
            link_or_copy(&obj_path, &snap_obj).map_err(|e| e.to_string())?;
            *reused += 1;
            rec
        } else {
            // Parse this exact content, store it content-addressed, link it in.
            let abs = view.canonical_root.join(&vf.relative_path);
            let read = crate::input::read_bounded(&abs, analyzer.config().max_file_size);
            let language =
                LanguageId::from_extension(vf.relative_path.rsplit('.').next().unwrap_or(""))
                    .ok_or_else(|| format!("unsupported language for {}", vf.relative_path))?;
            let analysis = build::analyze(analyzer, &vf.relative_path, language, &read);
            let rec = build::index_file(
                &analysis,
                &vf.content_digest,
                vf.source_bytes,
                read.truncated,
            );
            artifact::write_file_artifact(&obj_root, &objkey, &analysis)
                .map_err(|e| e.to_string())?;
            link_or_copy(&obj_path, &snap_obj).map_err(|e| e.to_string())?;
            *parsed += 1;
            rec
        };
        accumulate(&mut m, &indexed);
        m.files.push(indexed);
    }
    m.refresh_snapshot_digest();
    artifact::write_manifest(snap_dir, &m).map_err(|e| e.to_string())?;
    Ok(m)
}

fn read_analysis(path: &Path) -> Result<crate::model::FileAnalysis, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("read {path:?}: {e}"))?;
    serde_json::from_str(&text).map_err(|e| format!("parse {path:?}: {e}"))
}

fn accumulate(m: &mut RepositoryManifest, f: &crate::repository::manifest::IndexedFile) {
    match f.analysis_status.as_str() {
        "clean" => m.coverage.clean += 1,
        "recovered" => m.coverage.recovered += 1,
        "incomplete" => m.coverage.incomplete += 1,
        "failed" => m.coverage.failed += 1,
        "unsupported" => m.coverage.skipped += 1,
        _ => {}
    }
    m.totals.declarations += f.declarations;
    m.totals.imports += f.imports;
    m.totals.references += f.references;
    m.totals.call_like += f.call_like;
    m.totals.test_candidates += f.test_candidates;
    m.totals.source_bytes += f.source_bytes;
    m.coverage.supported_files += 1;
    m.coverage.visited_files += 1;
}

/// Ensure a derived index exists for `view`'s current content. Recomputes the
/// effective manifest (dirty-visible), then reuses or builds the
/// digest-keyed index.
pub fn ensure_index(
    analyzer: &Analyzer,
    view: &mut RepositoryView,
    state_override: Option<&Path>,
) -> Result<EnsureOutcome, String> {
    let started = Instant::now();
    let state = state_override
        .map(|p| p.to_path_buf())
        .unwrap_or_else(state_dir);
    let cache = manifest::view_cache_file(&state, &view.canonical_root);
    let manifest = manifest::compute(&view.canonical_root, Some(&cache))
        .map_err(|e| format!("compute view manifest: {e}"))?;
    let fingerprint = manifest.fingerprint();
    view.view_fingerprint = Some(fingerprint.clone());

    let index_dir = indexes_dir(&state).join(&fingerprint[7..23]);
    let graph_dir = index_dir.join("graph");
    if graph_dir.join("manifest.json").exists() {
        return Ok(EnsureOutcome {
            graph_dir,
            fingerprint,
            index_reused: true,
            analyses_reused: 0,
            analyses_parsed: 0,
            files: manifest.files.len() as u64,
            ensure_ms: started.elapsed().as_secs_f64() * 1000.0,
        });
    }

    // Acquire a per-fingerprint build lock so concurrent processes don't race on
    // the same derived index (§37). Build into a staging dir, then publish by
    // atomic rename — readers only ever see a complete index or none.
    let lock = match acquire_lock(&index_dir)? {
        // Another process finished it while we waited.
        None => {
            return Ok(EnsureOutcome {
                graph_dir,
                fingerprint,
                index_reused: true,
                analyses_reused: 0,
                analyses_parsed: 0,
                files: manifest.files.len() as u64,
                ensure_ms: started.elapsed().as_secs_f64() * 1000.0,
            })
        }
        Some(l) => l,
    };
    let staging = index_dir.with_extension(format!("build-{}", std::process::id()));
    let build_res = (|| -> Result<(u64, u64), String> {
        let _ = fs::remove_dir_all(&staging);
        let snap_dir = staging.join("snapshot");
        let mut reused = 0u64;
        let mut parsed = 0u64;
        build_view_snapshot(
            analyzer,
            view,
            &manifest,
            &snap_dir,
            &state,
            &mut reused,
            &mut parsed,
        )?;
        let links_dir = staging.join("links");
        crate::links::build::build_links(&snap_dir, Some(&view.canonical_root), &links_dir)
            .map_err(|e| e.to_string())?;
        let cand_dir = staging.join("candidates");
        crate::candidates::build::build_candidates(&snap_dir, &links_dir, &cand_dir)
            .map_err(|e| e.to_string())?;
        crate::graph::build::build_graph(&snap_dir, &links_dir, &cand_dir, &staging.join("graph"))
            .map_err(|e| e.to_string())?;
        Ok((reused, parsed))
    })();
    let (reused, parsed) = match build_res {
        Ok(x) => x,
        Err(e) => {
            let _ = fs::remove_dir_all(&staging);
            release_lock(&lock);
            return Err(e);
        }
    };
    // Publish: rename staging -> index_dir. If another process already published
    // a complete index, discard ours (identical content anyway).
    if index_dir.join("graph").join("manifest.json").exists() {
        let _ = fs::remove_dir_all(&staging);
    } else {
        let _ = fs::remove_dir_all(&index_dir);
        if fs::rename(&staging, &index_dir).is_err() {
            let _ = fs::remove_dir_all(&staging);
        }
    }
    release_lock(&lock);

    Ok(EnsureOutcome {
        graph_dir,
        fingerprint,
        index_reused: false,
        analyses_reused: reused,
        analyses_parsed: parsed,
        files: manifest.files.len() as u64,
        ensure_ms: started.elapsed().as_secs_f64() * 1000.0,
    })
}

/// Lockfile guard released on drop.
struct BuildLock {
    path: PathBuf,
}
impl Drop for BuildLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// Acquire `{index_dir}.lock` via create-new; waits for an in-flight build to
/// publish, and clears stale locks (>300s, crashed builder).
/// Returns `Ok(None)` when the index is already complete (no lock needed —
/// caller reuses it); `Ok(Some(guard))` when we hold the lock and must build.
fn acquire_lock(index_dir: &Path) -> Result<Option<BuildLock>, String> {
    let lock = index_dir.with_extension("lock");
    if let Some(d) = lock.parent() {
        let _ = fs::create_dir_all(d);
    }
    let deadline = Instant::now() + std::time::Duration::from_secs(300);
    loop {
        match fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&lock)
        {
            Ok(_) => return Ok(Some(BuildLock { path: lock })),
            Err(_) => {
                if index_dir.join("graph").join("manifest.json").exists() {
                    return Ok(None);
                }
                let stale = fs::metadata(&lock)
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|t| t.elapsed().ok())
                    .map(|e| e.as_secs() > 300)
                    .unwrap_or(false);
                if stale {
                    let _ = fs::remove_file(&lock);
                }
                if Instant::now() > deadline {
                    return Err("timed out waiting for concurrent index build".to_string());
                }
                std::thread::sleep(std::time::Duration::from_millis(150));
            }
        }
    }
}

fn release_lock(lock: &BuildLock) {
    let _ = fs::remove_file(&lock.path);
}
