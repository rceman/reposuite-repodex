//! Full repository snapshot build and incremental file-level update.
//!
//! ```text
//! repository
//!     -> deterministic discovery (scanner::walk_files)
//!     -> per-file bounded read + SHA-256 content digest
//!     -> reuse previous normalized analysis OR parse + extract the whole file
//!     -> RepositoryManifest + one artifact per indexed file
//!     -> verified atomic publication
//! ```
//!
//! The unit of work is the whole file. Unchanged files reuse their persisted
//! normalized analysis; changed and added files are parsed and extracted in
//! full. There is no fine-grained fact patching and Tree-sitter changed ranges
//! are never the invalidation mechanism.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::input::{self, InputError};
use crate::model::{FileAnalysis, LanguageId};
use crate::parser::Analyzer;
use crate::paths::relative_path;
use crate::repository::artifact::{self, SnapshotError};
use crate::repository::digest;
use crate::repository::fingerprint::{AnalyzerFingerprint, SnapshotConfig};
use crate::repository::manifest::{CoverageSummary, FactTotals, IndexedFile, RepositoryManifest};
use crate::scanner::{walk_files_excluding, ScanOptions};

/// Options for one build or update.
#[derive(Debug, Clone, Copy)]
pub struct BuildOptions {
    /// Honour `.gitignore` files inside the repository root.
    pub respect_gitignore: bool,
    /// Proceed even when a previous snapshot was produced by an incompatible
    /// analyzer or configuration, rebuilding every file instead of refusing.
    pub allow_incompatible: bool,
}

impl Default for BuildOptions {
    fn default() -> Self {
        Self {
            respect_gitignore: true,
            allow_incompatible: false,
        }
    }
}

/// Work actually performed by a build or update.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BuildStats {
    /// `full` or `update`.
    pub mode: &'static str,
    /// Indexed files in the resulting snapshot.
    pub files_total: u64,
    /// Files whose persisted analysis was reused without parsing.
    pub reused_files: u64,
    /// Files that were parsed and extracted in this run.
    pub reparsed_files: u64,
    /// Files re-extracted in this run. Always equals `reparsed_files` because
    /// extraction is whole-file; recorded separately to make the gate explicit.
    pub reextracted_files: u64,
    /// Current paths that were absent from the previous snapshot.
    pub added: u64,
    /// Current paths present in the previous snapshot with different content.
    pub changed: u64,
    /// Current paths present in the previous snapshot with identical content.
    pub unchanged: u64,
    /// Previous paths that no longer exist.
    pub deleted: u64,
    /// Source bytes read and hashed in this run.
    pub bytes_hashed: u64,
    /// Source bytes parsed and extracted in this run.
    pub bytes_reparsed: u64,
    /// Wall-clock duration of the build, in milliseconds.
    pub duration_ms: f64,
}

/// Result of a successful build or update.
#[derive(Debug, Clone)]
pub struct BuildOutcome {
    pub manifest: RepositoryManifest,
    pub stats: BuildStats,
}

/// Build a complete snapshot of `root` into `output`.
pub fn build_snapshot(
    analyzer: &Analyzer,
    root: &Path,
    output: &Path,
    options: BuildOptions,
) -> Result<BuildOutcome, SnapshotError> {
    run(analyzer, root, output, options, None)
}

/// Rebuild `output` from `previous`, reusing unchanged files.
pub fn update_snapshot(
    analyzer: &Analyzer,
    root: &Path,
    previous: &Path,
    output: &Path,
    options: BuildOptions,
) -> Result<BuildOutcome, SnapshotError> {
    run(analyzer, root, output, options, Some(previous))
}

fn run(
    analyzer: &Analyzer,
    root: &Path,
    output: &Path,
    options: BuildOptions,
    previous: Option<&Path>,
) -> Result<BuildOutcome, SnapshotError> {
    let started = Instant::now();
    let max_file_size = analyzer.config().max_file_size;
    let fingerprint = AnalyzerFingerprint::current(analyzer.registry());
    let config = SnapshotConfig::new(max_file_size, options.respect_gitignore);

    let mut stats = BuildStats {
        mode: if previous.is_some() { "update" } else { "full" },
        ..BuildStats::default()
    };

    // Load and validate the previous snapshot, if any, before touching output.
    let previous_manifest = match previous {
        Some(dir) => Some(load_previous(
            dir,
            &fingerprint,
            &config,
            &options,
            &mut stats,
        )?),
        None => None,
    };

    let staging = artifact::prepare_staging(output)?;
    let exclusions = exclusions_for(output, &staging, previous);
    let result = run_into(
        analyzer,
        root,
        &staging,
        &exclusions,
        &fingerprint,
        &config,
        previous,
        previous_manifest.as_ref(),
        &mut stats,
        started,
    );
    match result {
        Ok(manifest) => {
            let outcome = BuildOutcome {
                manifest,
                stats: stats.clone(),
            };
            artifact::publish(&staging, output)?;
            Ok(outcome)
        }
        Err(error) => {
            // Never leave a half-written staging directory behind.
            let _ = std::fs::remove_dir_all(&staging);
            Err(error)
        }
    }
}

/// Validate a previous snapshot and decide whether reuse is permitted.
fn load_previous(
    dir: &Path,
    fingerprint: &AnalyzerFingerprint,
    config: &SnapshotConfig,
    options: &BuildOptions,
    stats: &mut BuildStats,
) -> Result<RepositoryManifest, SnapshotError> {
    let manifest =
        artifact::read_manifest(dir).map_err(|error| SnapshotError::PreviousSnapshotUnusable {
            reason: error.to_string(),
        })?;

    let fingerprint_ok = manifest.fingerprint_matches(fingerprint);
    let config_ok = manifest.config_matches(config);
    if !fingerprint_ok || !config_ok {
        if !options.allow_incompatible {
            return Err(if !fingerprint_ok {
                SnapshotError::FingerprintMismatch {
                    previous: manifest.analyzer_fingerprint.clone(),
                    current: fingerprint.digest.clone(),
                }
            } else {
                SnapshotError::ConfigMismatch {
                    previous: manifest.config.canonical_text(),
                    current: config.canonical_text(),
                }
            });
        }
        // Explicitly allowed: fall through with reuse disabled. Recorded by the
        // caller through `reuse_enabled`.
        stats.mode = "update";
    }
    Ok(manifest)
}

/// True when a previous snapshot may be reused for `fingerprint`/`config`.
fn reuse_enabled(
    manifest: &RepositoryManifest,
    fingerprint: &AnalyzerFingerprint,
    config: &SnapshotConfig,
) -> bool {
    manifest.fingerprint_matches(fingerprint) && manifest.config_matches(config)
}

#[allow(clippy::too_many_arguments)]
fn run_into(
    analyzer: &Analyzer,
    root: &Path,
    staging: &Path,
    exclusions: &[PathBuf],
    fingerprint: &AnalyzerFingerprint,
    config: &SnapshotConfig,
    previous_dir: Option<&Path>,
    previous_manifest: Option<&RepositoryManifest>,
    stats: &mut BuildStats,
    started: Instant,
) -> Result<RepositoryManifest, SnapshotError> {
    let max_file_size = analyzer.config().max_file_size;
    let mut manifest = RepositoryManifest::new(fingerprint, config.clone());

    // Index the previous file records by path for O(1) lookup.
    let previous_files: BTreeMap<&str, &IndexedFile> = previous_manifest
        .map(|manifest| {
            manifest
                .files
                .iter()
                .map(|file| (file.relative_path.as_str(), file))
                .collect()
        })
        .unwrap_or_default();
    let can_reuse = previous_manifest
        .map(|manifest| reuse_enabled(manifest, fingerprint, config))
        .unwrap_or(false)
        && previous_dir.is_some();

    // The walk root is canonicalised so that the exclusion prefixes computed by
    // `exclusions_for` compare against the same absolute spelling.
    let walk_root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let (paths, traversal_failures) = walk_files_excluding(
        &walk_root,
        ScanOptions {
            respect_gitignore: config.respect_gitignore,
        },
        exclusions,
    )
    .map_err(|error| SnapshotError::Io {
        path: root.to_path_buf(),
        message: error.to_string(),
    })?;

    let mut coverage = CoverageSummary {
        visited_files: paths.len() as u64,
        traversal_failures: traversal_failures.len() as u64,
        scan_complete: traversal_failures.is_empty(),
        ..CoverageSummary::default()
    };
    let mut totals = FactTotals::default();
    let mut current_paths = Vec::new();

    for path in &paths {
        let language = path
            .extension()
            .and_then(|extension| extension.to_str())
            .and_then(LanguageId::from_extension);
        let Some(language) = language else {
            coverage.unsupported_files += 1;
            continue;
        };
        coverage.supported_files += 1;
        let relative = relative_path(&walk_root, path);
        current_paths.push(relative.clone());

        let read = input::read_bounded(path, max_file_size);
        let content = digest::content_digest(&read.bytes);
        let source_bytes = read.bytes.len() as u64;
        stats.bytes_hashed += source_bytes;

        let previous_file = previous_files.get(relative.as_str()).copied();
        let content_identical = previous_file
            .map(|file| {
                file.content_digest == content
                    && file.source_bytes == source_bytes
                    && file.source_truncated == read.truncated
                    && file.language == language.as_str()
            })
            .unwrap_or(false);

        if previous_file.is_some() {
            if content_identical {
                stats.unchanged += 1;
            } else {
                stats.changed += 1;
            }
        } else {
            stats.added += 1;
        }

        let indexed = if can_reuse && content_identical {
            let previous_file = previous_file.expect("content match implies a previous record");
            let previous_dir = previous_dir.expect("reuse implies a previous directory");
            artifact::copy_file_artifact(previous_dir, staging, &previous_file.object_key)?;
            stats.reused_files += 1;
            previous_file.clone()
        } else {
            let analysis = analyze(analyzer, &relative, language, &read);
            let indexed = index_file(&analysis, &content, source_bytes, read.truncated);
            artifact::write_file_artifact(staging, &indexed.object_key, &analysis)?;
            stats.reparsed_files += 1;
            stats.reextracted_files += 1;
            stats.bytes_reparsed += source_bytes;
            indexed
        };

        accumulate(&mut coverage, &mut totals, &indexed);
        manifest.files.push(indexed);
    }

    stats.files_total = manifest.files.len() as u64;
    stats.deleted = previous_files
        .keys()
        .filter(|path| !current_paths.iter().any(|current| current == *path))
        .count() as u64;

    // `walk_files` returns paths sorted by relative path, so the manifest file
    // list is already in canonical order.
    manifest.coverage = coverage;
    manifest.totals = totals;
    manifest.refresh_snapshot_digest();
    artifact::write_manifest(staging, &manifest)?;

    stats.duration_ms = started.elapsed().as_secs_f64() * 1000.0;
    Ok(manifest)
}

/// Analyze one file from bytes already read.
pub(crate) fn analyze(
    analyzer: &Analyzer,
    relative: &str,
    language: LanguageId,
    read: &input::BoundedRead,
) -> FileAnalysis {
    match &read.error {
        // A file that could not be read at all has no bytes to analyze; the
        // analysis records the failure with an empty source.
        Some(InputError::NotFound) | Some(InputError::Io(_)) => {
            let error = read.error.as_ref().expect("matched above");
            FileAnalysis::unsupported(relative, language, b"", error.status(), error.diagnostic())
        }
        // Over-limit and non-UTF-8 inputs keep their bytes, so they go through
        // the same validation path a scan would use.
        _ => analyzer.analyze_bytes(relative, language, &read.bytes),
    }
}

/// Build the manifest record for one analyzed file.
pub(crate) fn index_file(
    analysis: &FileAnalysis,
    content_digest: &str,
    source_bytes: u64,
    source_truncated: bool,
) -> IndexedFile {
    IndexedFile {
        relative_path: analysis.file.relative_path.clone(),
        language: analysis.file.language.as_str().to_string(),
        source_bytes,
        source_truncated,
        content_digest: content_digest.to_string(),
        analysis_status: analysis.status.as_str().to_string(),
        analysis_digest: digest::analysis_digest(analysis),
        object_key: artifact::object_key(&analysis.file.relative_path, content_digest),
        diagnostics: analysis
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.render())
            .collect(),
        declarations: analysis.declarations.len() as u64,
        imports: analysis.imports.len() as u64,
        references: analysis.references.len() as u64,
        call_like: analysis.calls.len() as u64,
        test_candidates: analysis.test_candidate_count() as u64,
    }
}

fn accumulate(coverage: &mut CoverageSummary, totals: &mut FactTotals, file: &IndexedFile) {
    match file.analysis_status.as_str() {
        "clean" => coverage.clean += 1,
        "recovered" => coverage.recovered += 1,
        "incomplete" => coverage.incomplete += 1,
        "failed" => coverage.failed += 1,
        "unsupported" => coverage.skipped += 1,
        _ => {}
    }
    totals.declarations += file.declarations;
    totals.imports += file.imports;
    totals.references += file.references;
    totals.call_like += file.call_like;
    totals.test_candidates += file.test_candidates;
    totals.source_bytes += file.source_bytes;
}

/// Convenience wrapper: load a snapshot and return its manifest.
pub fn load_manifest(dir: &Path) -> Result<RepositoryManifest, SnapshotError> {
    artifact::read_manifest(dir)
}

/// Convenience wrapper: fully verify a snapshot.
pub fn verify_snapshot(dir: &Path) -> Result<artifact::VerificationReport, SnapshotError> {
    artifact::verify(dir)
}

/// Directories a snapshot build must never index.
///
/// A snapshot describes the repository's source, not its own output. When the
/// output directory happens to live inside the repository root, indexing it
/// would make the manifest's discovery counters depend on where the artifact is
/// written, and would let a snapshot ingest a previous snapshot's JSON.
fn exclusions_for(output: &Path, staging: &Path, previous: Option<&Path>) -> Vec<PathBuf> {
    let mut excluded = vec![
        resolve_for_exclusion(output),
        resolve_for_exclusion(staging),
        resolve_for_exclusion(&artifact::backup_dir(output)),
    ];
    if let Some(previous) = previous {
        excluded.push(resolve_for_exclusion(previous));
    }
    excluded.sort();
    excluded.dedup();
    excluded
}

/// Absolute, symlink-resolved spelling of a path that may not exist yet.
///
/// The walk root is canonicalised, so exclusions have to be canonicalised the
/// same way or the prefix comparison would miss a symlinked checkout.
fn resolve_for_exclusion(path: &Path) -> PathBuf {
    if let Ok(canonical) = std::fs::canonicalize(path) {
        return canonical;
    }
    let name = path.file_name().map(PathBuf::from);
    let parent = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
    };
    let parent = std::fs::canonicalize(&parent).unwrap_or(parent);
    match name {
        Some(name) => parent.join(name),
        None => parent,
    }
}
