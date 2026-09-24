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
use crate::repository::digest;
use crate::repository::fingerprint::AnalyzerFingerprint;
use crate::repository::manifest::RepositoryManifest;

use super::manifest::{self, ViewManifest};
use super::model::RepositoryView;
use super::registry::state_dir;
use super::validity::{self, ValidityInputs};

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

/// Did `vf` change since the manifest was computed? Compare the live stat
/// hint to the recorded one — cheap, no re-read (§14-§15 coherent capture).
fn file_changed_since_manifest(view: &RepositoryView, vf: &super::manifest::ViewFile) -> bool {
    let abs = view.canonical_root.join(&vf.relative_path);
    let (mtime_ns, change_ns, size) = manifest::stat_key(&abs);
    !(mtime_ns == vf.mtime_ns && change_ns == vf.change_ns && size == vf.size)
}

/// Build the view's snapshot into `snap_dir`, reusing shared FileAnalysis
/// objects by an analyzer-scoped `object_key(path, digest, analyzer)` (§27, §11).
///
/// Coherent capture (§14-§15): the recorded digest is ALWAYS the digest of the
/// bytes actually read. On the reuse path the manifest digest is trusted only
/// when the file's stat hint is unchanged since manifest; on the parse path the
/// digest is recomputed from the bytes read. Returns the manifest + the
/// *actual* captured content fingerprint (recomputed from the digests used) +
/// a flag noting whether any file diverged from the precomputed manifest.
/// Mutable counters reported by `build_view_snapshot`.
#[derive(Default)]
struct CaptureStats {
    reused: u64,
    parsed: u64,
    /// A file changed between manifest compute and capture (§14-§16).
    diverged: bool,
}

fn build_view_snapshot(
    analyzer: &Analyzer,
    view: &RepositoryView,
    manifest: &ViewManifest,
    snap_dir: &Path,
    state: &Path,
    stats: &mut CaptureStats,
) -> Result<(RepositoryManifest, String), String> {
    let fingerprint = AnalyzerFingerprint::current(analyzer.registry());
    let config =
        crate::repository::fingerprint::SnapshotConfig::new(analyzer.config().max_file_size, true);
    let mut m = RepositoryManifest::new(&fingerprint, config);
    let analyzer_digest = fingerprint.digest.clone();
    let obj_root = objects_dir(state);
    fs::create_dir_all(obj_root.join("files")).map_err(|e| e.to_string())?;
    let snap_files = snap_dir.join("files");
    fs::create_dir_all(&snap_files).map_err(|e| e.to_string())?;

    // Rebuild the content fingerprint from the digests actually used so the
    // published index key always describes the captured bytes (§14-§16).
    let mut actual_seed = String::new();
    for vf in &manifest.files {
        // Trust the manifest digest for reuse ONLY if the file is stat-stable
        // since manifest; a mid-build change forces a re-read+rehash.
        let stable = !file_changed_since_manifest(view, vf);
        let objkey =
            artifact::object_key_scoped(&vf.relative_path, &vf.content_digest, &analyzer_digest);
        let obj_path = obj_root.join("files").join(format!("{objkey}.json"));
        let snap_obj = snap_files.join(format!("{objkey}.json"));
        let (indexed, used_digest) = if stable && obj_path.exists() {
            let analysis = read_analysis(&obj_path)?;
            let mut rec = build::index_file(&analysis, &vf.content_digest, vf.source_bytes, false);
            rec.object_key = objkey.clone(); // manifest references the scoped key
            link_or_copy(&obj_path, &snap_obj).map_err(|e| e.to_string())?;
            stats.reused += 1;
            (rec, vf.content_digest.clone())
        } else {
            // Parse this exact content; the recorded digest is of the bytes read.
            let abs = view.canonical_root.join(&vf.relative_path);
            let read = crate::input::read_bounded(&abs, analyzer.config().max_file_size);
            let actual_digest = digest::content_digest(&read.bytes);
            if actual_digest != vf.content_digest {
                stats.diverged = true; // file changed between manifest and capture
            }
            let language =
                LanguageId::from_extension(vf.relative_path.rsplit('.').next().unwrap_or(""))
                    .ok_or_else(|| format!("unsupported language for {}", vf.relative_path))?;
            let analysis = build::analyze(analyzer, &vf.relative_path, language, &read);
            let realkey =
                artifact::object_key_scoped(&vf.relative_path, &actual_digest, &analyzer_digest);
            let real_obj = obj_root.join("files").join(format!("{realkey}.json"));
            artifact::write_file_artifact(&obj_root, &realkey, &analysis)
                .map_err(|e| e.to_string())?;
            link_or_copy(&real_obj, &snap_obj).map_err(|e| e.to_string())?;
            let mut rec =
                build::index_file(&analysis, &actual_digest, vf.source_bytes, read.truncated);
            rec.object_key = realkey;
            stats.parsed += 1;
            (rec, actual_digest)
        };
        actual_seed.push_str(&format!(
            "file path={} lang={} content={}\n",
            vf.relative_path, vf.language, used_digest
        ));
        accumulate(&mut m, &indexed);
        m.files.push(indexed);
    }
    // Metadata inputs are part of the captured content identity (§4-§6).
    for meta in &manifest.metadata {
        actual_seed.push_str(&format!(
            "meta path={} content={}\n",
            meta.relative_path, meta.content_digest
        ));
    }
    m.refresh_snapshot_digest();
    artifact::write_manifest(snap_dir, &m).map_err(|e| e.to_string())?;
    Ok((m, digest::content_digest(actual_seed.as_bytes())))
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

/// Is `index_dir` a complete, currently-valid index for `inputs`? A directory
/// hit alone is insufficient — the recorded validity inputs must match (§17).
fn index_valid(index_dir: &Path, inputs: &ValidityInputs) -> bool {
    if !index_dir.join("graph").join("manifest.json").exists() {
        return false;
    }
    match ValidityInputs::read(index_dir) {
        Some(stored) => ValidityInputs::validate(&stored, inputs).is_none(),
        // No validity record -> treat as not provably valid (rebuild once to
        // stamp it). This is a conservative, one-time migration for indexes
        // written before the gate existed.
        None => false,
    }
}

/// Ensure a derived index exists for `view`'s current content. Recomputes the
/// effective manifest (dirty-visible, metadata-aware), keys the index on the
/// full validity inputs, then reuses or builds (§17-§19, §26).
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
    let content_fp = manifest.fingerprint();
    view.view_fingerprint = Some(content_fp.clone());

    // The validity key covers content AND metadata AND every producer/config
    // identity — a directory hit under it is provably valid (§17-§18).
    let inputs = ValidityInputs::current(analyzer, &manifest);
    let validity_key = inputs.key();
    let index_dir = validity::index_dir(&state, &validity_key);
    let graph_dir = index_dir.join("graph");
    if index_valid(&index_dir, &inputs) {
        return Ok(EnsureOutcome {
            graph_dir,
            fingerprint: content_fp,
            index_reused: true,
            analyses_reused: 0,
            analyses_parsed: 0,
            files: manifest.files.len() as u64,
            ensure_ms: started.elapsed().as_secs_f64() * 1000.0,
        });
    }

    // Acquire a per-key build lock so concurrent processes don't race (§37).
    let lock = match acquire_lock(&index_dir)? {
        None => {
            return Ok(EnsureOutcome {
                graph_dir,
                fingerprint: content_fp,
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
    let build_res = (|| -> Result<(u64, u64, String, bool), String> {
        let _ = fs::remove_dir_all(&staging);
        let snap_dir = staging.join("snapshot");
        let mut stats = CaptureStats::default();
        let (_m, actual_content_fp) =
            build_view_snapshot(analyzer, view, &manifest, &snap_dir, &state, &mut stats)?;
        let (reused, parsed, diverged) = (stats.reused, stats.parsed, stats.diverged);
        let links_dir = staging.join("links");
        crate::links::build::build_links(&snap_dir, Some(&view.canonical_root), &links_dir)
            .map_err(|e| e.to_string())?;
        let cand_dir = staging.join("candidates");
        crate::candidates::build::build_candidates(&snap_dir, &links_dir, &cand_dir)
            .map_err(|e| e.to_string())?;
        crate::graph::build::build_graph(&snap_dir, &links_dir, &cand_dir, &staging.join("graph"))
            .map_err(|e| e.to_string())?;
        Ok((reused, parsed, actual_content_fp, diverged))
    })();
    let (reused, parsed, actual_content_fp, diverged) = match build_res {
        Ok(x) => x,
        Err(e) => {
            let _ = fs::remove_dir_all(&staging);
            release_lock(&lock);
            return Err(e);
        }
    };
    // Publish under the key for the ACTUAL captured inputs (coherent: if a file
    // changed mid-build, the index is keyed on the bytes really used, §14-§16).
    let publish_inputs = if actual_content_fp == content_fp && !diverged {
        inputs.clone()
    } else {
        let mut i = inputs.clone();
        i.content_fingerprint = actual_content_fp.clone();
        i
    };
    let publish_dir = validity::index_dir(&state, &publish_inputs.key());
    let publish_lock = if publish_dir == index_dir {
        lock
    } else {
        release_lock(&lock);
        match acquire_lock(&publish_dir)? {
            None => {
                return Ok(reused_outcome(
                    &publish_dir.join("graph"),
                    actual_content_fp,
                    &manifest,
                    started,
                ))
            }
            Some(l) => l,
        }
    };
    // Record the validity inputs inside the index before publishing.
    if let Err(e) = publish_inputs.write(&staging) {
        let _ = fs::remove_dir_all(&staging);
        release_lock(&publish_lock);
        return Err(e);
    }
    if index_valid(&publish_dir, &publish_inputs) {
        let _ = fs::remove_dir_all(&staging);
    } else {
        let _ = fs::remove_dir_all(&publish_dir);
        if fs::rename(&staging, &publish_dir).is_err() {
            let _ = fs::remove_dir_all(&staging);
        }
    }
    release_lock(&publish_lock);

    Ok(EnsureOutcome {
        graph_dir: publish_dir.join("graph"),
        fingerprint: actual_content_fp,
        index_reused: false,
        analyses_reused: reused,
        analyses_parsed: parsed,
        files: manifest.files.len() as u64,
        ensure_ms: started.elapsed().as_secs_f64() * 1000.0,
    })
}

fn reused_outcome(
    graph_dir: &Path,
    fingerprint: String,
    manifest: &ViewManifest,
    started: Instant,
) -> EnsureOutcome {
    EnsureOutcome {
        graph_dir: graph_dir.to_path_buf(),
        fingerprint,
        index_reused: true,
        analyses_reused: 0,
        analyses_parsed: 0,
        files: manifest.files.len() as u64,
        ensure_ms: started.elapsed().as_secs_f64() * 1000.0,
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::AnalyzerConfig;

    // §14-§16 coherent capture: a manifest that records digest A while the file
    // actually holds bytes B must produce an artifact bound to B's digest — the
    // index can never record "digest A + analysis of B" (§15).
    #[test]
    fn capture_binds_to_bytes_actually_read() {
        let tmp = std::env::temp_dir().join(format!("vv-cap-{}", std::process::id()));
        let root = tmp.join("repo");
        fs::create_dir_all(&root).unwrap();
        let state = tmp.join("state");
        fs::create_dir_all(&state).unwrap();
        // The file holds bytes B.
        fs::write(root.join("a.go"), "package a\nfunc A() int { return 2 }\n").unwrap();
        let analyzer = Analyzer::new(AnalyzerConfig {
            max_file_size: u64::MAX,
        })
        .unwrap();
        let view = RepositoryView {
            repository_id: "r".into(),
            view_id: "v".into(),
            canonical_root: root.clone(),
            head: None,
            branch: None,
            detached: false,
            dirty: false,
            git_common_dir: None,
            view_fingerprint: None,
            locator: "root".into(),
        };
        // Forge a STALE manifest: claims a.go has digest-of-A + a stat that no
        // longer matches the live file (divergence must be detected).
        let stale = ViewManifest {
            files: vec![super::manifest::ViewFile {
                relative_path: "a.go".into(),
                language: "go".into(),
                source_bytes: 0,
                content_digest: digest::content_digest(b"package a\nfunc A() int { return 1 }\n"),
                mtime_secs: 0,
                mtime_ns: 0,
                change_ns: 0,
                size: 0,
            }],
            metadata: vec![],
        };
        let mut stats = CaptureStats::default();
        let snap = tmp.join("snap");
        let (m, actual_fp) =
            build_view_snapshot(&analyzer, &view, &stale, &snap, &state, &mut stats).unwrap();
        let diverged = stats.diverged;
        let actual_b = digest::content_digest(b"package a\nfunc A() int { return 2 }\n");
        assert_eq!(
            m.files[0].content_digest, actual_b,
            "artifact must record the digest of bytes actually read (B), not the stale A"
        );
        assert!(
            diverged,
            "a changed file between manifest and capture is flagged"
        );
        assert_ne!(
            actual_fp,
            stale.fingerprint(),
            "the published fingerprint reflects the captured bytes, not the stale manifest"
        );
        let _ = fs::remove_dir_all(&tmp);
        let _ = view;
    }
}
