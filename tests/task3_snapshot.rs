//! TASK 3A: deterministic repository fact snapshot and incremental file-level
//! index.
//!
//! These tests are the acceptance gates for the repository layer. The two that
//! matter most are the no-change gate (every file reused, nothing reparsed, and
//! the result equals a fresh build) and the one-file-change gate (exactly one
//! file reparsed, and the result equals a fresh build of the final state).

mod support;

use std::path::{Path, PathBuf};

use repodex::repository::{
    build_snapshot, load_manifest, update_snapshot, BuildOptions, BuildStats, RepositoryFactIndex,
    RepositoryManifest, SnapshotError,
};
use repodex::{Analyzer, AnalyzerConfig, LanguageId};
use support::TempDir;

fn write(root: &Path, relative: &str, contents: &[u8]) {
    let target = root.join(relative);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).expect("create parent");
    }
    std::fs::write(target, contents).expect("write file");
}

/// A synthetic four-language repository.
fn scaffold(temp: &TempDir) -> PathBuf {
    let root = temp.path().join("repo");
    write(
        &root,
        "src/lib.rs",
        b"pub fn alpha() {}\nfn beta() { alpha(); }\n",
    );
    write(&root, "src/other.rs", b"pub fn gamma() { beta(); }\n");
    write(
        &root,
        "main.go",
        b"package main\n\nfunc main() { helper() }\nfunc helper() {}\n",
    );
    write(
        &root,
        "app.py",
        b"def main():\n    helper()\n\ndef helper():\n    pass\n",
    );
    write(
        &root,
        "index.php",
        b"<?php\nfunction main() { helper(); }\nfunction helper() {}\n",
    );
    // A file whose syntax does not parse cleanly, so the snapshot has to carry a
    // recovered status rather than pretending everything is clean.
    write(&root, "src/broken.rs", b"fn broken( {\n    let x = ;\n}\n");
    // Not a supported language: must never appear in the index.
    write(&root, "README.md", b"# not indexed\n");
    root
}

/// A snapshot location outside the repository root, which is the normal layout.
fn store(temp: &TempDir, name: &str) -> PathBuf {
    temp.path().join("store").join(name)
}

fn analyzer() -> Analyzer {
    Analyzer::new(AnalyzerConfig::default()).expect("analyzer")
}

fn analyzer_with_limit(limit: u64) -> Analyzer {
    Analyzer::new(AnalyzerConfig {
        max_file_size: limit,
    })
    .expect("analyzer")
}

fn build(root: &Path, output: &Path) -> RepositoryManifest {
    build_snapshot(&analyzer(), root, output, BuildOptions::default())
        .expect("build")
        .manifest
}

fn update(root: &Path, previous: &Path, output: &Path) -> (RepositoryManifest, BuildStats) {
    let outcome = update_snapshot(&analyzer(), root, previous, output, BuildOptions::default())
        .expect("update");
    (outcome.manifest, outcome.stats)
}

/// Rewrite a snapshot's manifest in place, keeping it internally valid.
fn mutate_manifest(dir: &Path, mutate: impl FnOnce(&mut RepositoryManifest)) {
    let mut manifest = load_manifest(dir).expect("load manifest");
    mutate(&mut manifest);
    manifest.refresh_snapshot_digest();
    std::fs::write(
        dir.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest).expect("serialize"),
    )
    .expect("write manifest");
}

/// Rewrite a snapshot's manifest without refreshing the snapshot digest, so the
/// artifact is internally inconsistent.
fn mutate_manifest_stale_digest(dir: &Path, mutate: impl FnOnce(&mut RepositoryManifest)) {
    let mut manifest = load_manifest(dir).expect("load manifest");
    mutate(&mut manifest);
    std::fs::write(
        dir.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest).expect("serialize"),
    )
    .expect("write manifest");
}

fn first_object_key(dir: &Path) -> String {
    load_manifest(dir).expect("manifest").files[0]
        .object_key
        .clone()
}

// ---------------------------------------------------------------------------
// Fresh build
// ---------------------------------------------------------------------------

#[test]
fn a_fresh_build_indexes_supported_files_and_ignores_unsupported_ones() {
    let temp = TempDir::new("t3a-fresh");
    let root = scaffold(&temp);
    let manifest = build(&root, &store(&temp, "snap"));

    let paths: Vec<&str> = manifest
        .files
        .iter()
        .map(|file| file.relative_path.as_str())
        .collect();
    assert!(paths.contains(&"src/lib.rs"));
    assert!(paths.contains(&"main.go"));
    assert!(paths.contains(&"app.py"));
    assert!(paths.contains(&"index.php"));
    assert!(
        !paths.contains(&"README.md"),
        "an unsupported extension must not be indexed"
    );
    // Canonical ordering: strictly increasing relative paths.
    let mut sorted = paths.clone();
    sorted.sort_unstable();
    assert_eq!(
        paths, sorted,
        "manifest files must be in canonical path order"
    );

    assert_eq!(manifest.coverage.supported_files, 6);
    assert_eq!(manifest.coverage.unsupported_files, 1);
    assert!(manifest.coverage.scan_complete);
    assert!(manifest.totals.declarations > 0);
    assert!(manifest.snapshot_digest.starts_with("sha256:"));
}

#[test]
fn recovered_and_clean_files_keep_their_distinct_statuses() {
    let temp = TempDir::new("t3a-status");
    let root = scaffold(&temp);
    let manifest = build(&root, &store(&temp, "snap"));

    let broken = manifest.file("src/broken.rs").expect("broken file indexed");
    assert_eq!(broken.analysis_status, "recovered");
    let lib = manifest.file("src/lib.rs").expect("lib indexed");
    assert_eq!(lib.analysis_status, "clean");
    assert_eq!(manifest.coverage.recovered, 1);
    assert_eq!(manifest.coverage.clean, 5);
    // Every file produced a complete analysis, but the snapshot is not fully
    // clean because one file needed recovery.
    assert!(manifest.coverage.is_fully_analyzed());
    assert!(!manifest.coverage.is_fully_clean());
}

#[test]
fn the_snapshot_artifact_is_verifiable_and_round_trips() {
    let temp = TempDir::new("t3a-verify");
    let root = scaffold(&temp);
    let output = store(&temp, "snap");
    let manifest = build(&root, &output);

    let report = repodex::repository::verify(&output).expect("verify");
    assert_eq!(report.snapshot_digest, manifest.snapshot_digest);
    assert_eq!(report.files, manifest.files.len() as u64);
    assert_eq!(report.checked_artifacts, manifest.files.len() as u64);
    assert!(report.artifact_bytes > 0);

    // Every artifact deserializes back to the exact analysis that was indexed.
    let index = RepositoryFactIndex::load(&output).expect("load index");
    assert_eq!(index.files().len(), manifest.files.len());
    assert_eq!(
        index.file("src/lib.rs").expect("lib").file.language,
        LanguageId::Rust
    );
}

#[test]
fn the_persisted_artifact_holds_normalized_facts_not_source_buffers() {
    let temp = TempDir::new("t3a-no-source");
    let root = temp.path().join("repo");
    // A comment is never a normalized fact, so if the source buffer were
    // persisted the marker would show up in the artifact.
    write(
        &root,
        "src/lib.rs",
        b"// SOURCE_BUFFER_MARKER\npub fn alpha() {}\n",
    );
    let output = store(&temp, "snap");
    build(&root, &output);

    let key = first_object_key(&output);
    let artifact = std::fs::read_to_string(output.join("files").join(format!("{key}.json")))
        .expect("artifact");
    assert!(
        !artifact.contains("SOURCE_BUFFER_MARKER"),
        "a snapshot must not persist source buffers"
    );
    assert!(
        !artifact.contains("\"tree\""),
        "a snapshot must not persist Tree-sitter trees"
    );
}

// ---------------------------------------------------------------------------
// Determinism
// ---------------------------------------------------------------------------

#[test]
fn repeated_builds_produce_identical_digests() {
    let temp = TempDir::new("t3a-repeat");
    let root = scaffold(&temp);
    let first = build(&root, &store(&temp, "snap-a"));
    let second = build(&root, &store(&temp, "snap-b"));
    assert_eq!(first.snapshot_digest, second.snapshot_digest);
    assert_eq!(first, second);
}

#[test]
fn identical_content_under_different_roots_produces_identical_digests() {
    let first_temp = TempDir::new("t3a-root-a");
    let second_temp = TempDir::new("t3a-root-b");
    let first_root = scaffold(&first_temp);
    let second_root = scaffold(&second_temp);

    let first = build(&first_root, &store(&first_temp, "snap"));
    let second = build(&second_root, &store(&second_temp, "snap"));
    assert_eq!(
        first.snapshot_digest, second.snapshot_digest,
        "the canonical snapshot digest must not depend on the checkout root"
    );
    assert_eq!(first, second);
}

#[test]
fn the_output_directory_does_not_affect_the_canonical_digest() {
    let temp = TempDir::new("t3a-output");
    let root = scaffold(&temp);
    let nested = temp
        .path()
        .join("store")
        .join("deeply")
        .join("nested")
        .join("snap");
    let first = build(&root, &store(&temp, "snap"));
    let second = build(&root, &nested);
    assert_eq!(first.snapshot_digest, second.snapshot_digest);
    assert_eq!(first, second);
}

#[test]
fn the_manifest_contains_no_absolute_paths_or_timestamps() {
    let temp = TempDir::new("t3a-canonical");
    let root = scaffold(&temp);
    let output = store(&temp, "snap");
    build(&root, &output);
    let text = std::fs::read_to_string(output.join("manifest.json")).expect("manifest");
    let root_text = root.display().to_string();
    assert!(
        !text.contains(&root_text),
        "canonical content must not embed the checkout root"
    );
    for field in ["timestamp", "created_at", "generated_at", "uuid", "elapsed"] {
        assert!(
            !text.contains(field),
            "canonical manifest must not contain `{field}`"
        );
    }
}

#[test]
fn an_output_directory_inside_the_root_is_not_indexed() {
    let temp = TempDir::new("t3a-in-root-output");
    let root = scaffold(&temp);
    // The output lives inside the repository being snapshotted.
    let previous = root.join("snap");
    let output = root.join("snap2");

    let first = build(&root, &previous);
    assert_eq!(
        first.coverage.unsupported_files, 1,
        "the snapshot's own artifact must not be indexed as unsupported files"
    );

    let (updated, _) = update(&root, &previous, &output);
    assert_eq!(
        updated, first,
        "an in-root output must not change canonical snapshot content"
    );
    assert_eq!(updated.snapshot_digest, first.snapshot_digest);
}

// ---------------------------------------------------------------------------
// Mandatory incremental scenarios
// ---------------------------------------------------------------------------

#[test]
fn no_source_changes_reuses_every_file_and_equals_a_fresh_build() {
    let temp = TempDir::new("t3a-nochange");
    let root = scaffold(&temp);
    let previous = store(&temp, "previous");
    let output = store(&temp, "output");
    let fresh_before = build(&root, &previous);

    let (updated, stats) = update(&root, &previous, &output);

    assert_eq!(stats.mode, "update");
    assert_eq!(stats.reused_files, stats.files_total);
    assert_eq!(stats.reparsed_files, 0, "nothing may be reparsed");
    assert_eq!(stats.reextracted_files, 0, "nothing may be re-extracted");
    assert_eq!(stats.changed, 0);
    assert_eq!(stats.added, 0);
    assert_eq!(stats.deleted, 0);
    assert_eq!(stats.bytes_reparsed, 0);
    // Reading and hashing every file is expected work, not a bug.
    assert!(stats.bytes_hashed > 0);

    let fresh = build(&root, &store(&temp, "fresh"));
    assert_eq!(updated.snapshot_digest, fresh.snapshot_digest);
    assert_eq!(updated, fresh);
    assert_eq!(updated, fresh_before);
}

#[test]
fn one_changed_file_is_the_only_file_reparsed_and_equals_a_fresh_build() {
    let temp = TempDir::new("t3a-onechange");
    let root = scaffold(&temp);
    let previous = store(&temp, "previous");
    let output = store(&temp, "output");
    build(&root, &previous);

    write(
        &root,
        "src/other.rs",
        b"pub fn gamma() { alpha(); }\npub fn delta() {}\n",
    );

    let (updated, stats) = update(&root, &previous, &output);

    assert_eq!(stats.changed, 1);
    assert_eq!(stats.reparsed_files, 1, "exactly one file may be reparsed");
    assert_eq!(stats.reextracted_files, 1);
    assert_eq!(stats.reused_files, stats.files_total - 1);
    assert_eq!(stats.added, 0);
    assert_eq!(stats.deleted, 0);
    // Only the changed file's bytes are reparsed.
    let changed_bytes = updated.file("src/other.rs").expect("changed").source_bytes;
    assert_eq!(stats.bytes_reparsed, changed_bytes);

    let fresh = build(&root, &store(&temp, "fresh"));
    assert_eq!(updated.snapshot_digest, fresh.snapshot_digest);
    assert_eq!(updated, fresh);
    assert_eq!(
        updated.file("src/other.rs").expect("changed").declarations,
        2
    );
}

#[test]
fn multiple_changed_files_are_reparsed_and_the_rest_reused() {
    let temp = TempDir::new("t3a-multichange");
    let root = scaffold(&temp);
    let previous = store(&temp, "previous");
    let output = store(&temp, "output");
    build(&root, &previous);

    write(
        &root,
        "src/lib.rs",
        b"pub fn alpha() {}\npub fn extra() {}\n",
    );
    write(
        &root,
        "app.py",
        b"def main():\n    helper()\n\ndef helper():\n    pass\n\ndef extra():\n    pass\n",
    );

    let (updated, stats) = update(&root, &previous, &output);
    assert_eq!(stats.changed, 2);
    assert_eq!(stats.reparsed_files, 2);
    assert_eq!(stats.reused_files, stats.files_total - 2);

    let fresh = build(&root, &store(&temp, "fresh"));
    assert_eq!(updated.snapshot_digest, fresh.snapshot_digest);
    assert_eq!(updated, fresh);
}

#[test]
fn an_added_file_is_parsed_and_included() {
    let temp = TempDir::new("t3a-added");
    let root = scaffold(&temp);
    let previous = store(&temp, "previous");
    let output = store(&temp, "output");
    build(&root, &previous);

    write(&root, "src/added.rs", b"pub fn added() {}\n");

    let (updated, stats) = update(&root, &previous, &output);
    assert_eq!(stats.added, 1);
    assert_eq!(stats.changed, 0);
    assert_eq!(stats.reparsed_files, 1);
    assert_eq!(stats.reused_files, stats.files_total - 1);
    assert!(updated.file("src/added.rs").is_some());

    let fresh = build(&root, &store(&temp, "fresh"));
    assert_eq!(updated.snapshot_digest, fresh.snapshot_digest);
    assert_eq!(updated, fresh);
}

#[test]
fn a_deleted_file_is_removed() {
    let temp = TempDir::new("t3a-deleted");
    let root = scaffold(&temp);
    let previous = store(&temp, "previous");
    let output = store(&temp, "output");
    build(&root, &previous);

    std::fs::remove_file(root.join("app.py")).expect("remove");

    let (updated, stats) = update(&root, &previous, &output);
    assert_eq!(stats.deleted, 1);
    assert_eq!(stats.reparsed_files, 0);
    assert!(updated.file("app.py").is_none());

    let fresh = build(&root, &store(&temp, "fresh"));
    assert_eq!(updated.snapshot_digest, fresh.snapshot_digest);
    assert_eq!(updated, fresh);
}

#[test]
fn a_renamed_file_is_a_delete_plus_an_add() {
    let temp = TempDir::new("t3a-renamed");
    let root = scaffold(&temp);
    let previous = store(&temp, "previous");
    let output = store(&temp, "output");
    build(&root, &previous);

    let contents = std::fs::read(root.join("main.go")).expect("read");
    std::fs::remove_file(root.join("main.go")).expect("remove");
    write(&root, "cmd/main.go", &contents);

    let (updated, stats) = update(&root, &previous, &output);
    assert_eq!(stats.added, 1);
    assert_eq!(stats.deleted, 1);
    assert_eq!(stats.reparsed_files, 1, "the added path is parsed");
    assert!(updated.file("main.go").is_none());
    assert!(updated.file("cmd/main.go").is_some());

    let fresh = build(&root, &store(&temp, "fresh"));
    assert_eq!(updated.snapshot_digest, fresh.snapshot_digest);
    assert_eq!(updated, fresh);
}

#[test]
fn a_file_that_becomes_invalid_utf8_is_reindexed_with_a_skipped_status() {
    let temp = TempDir::new("t3a-utf8");
    let root = scaffold(&temp);
    let previous = store(&temp, "previous");
    let output = store(&temp, "output");
    build(&root, &previous);

    write(&root, "app.py", &[0x66, 0x6f, 0x6f, 0xff, 0xfe, 0x0a]);

    let (updated, stats) = update(&root, &previous, &output);
    assert_eq!(stats.changed, 1);
    assert_eq!(stats.reparsed_files, 1);
    let record = updated.file("app.py").expect("indexed");
    assert_eq!(record.analysis_status, "unsupported");
    assert!(record
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.contains("not valid UTF-8")));

    let fresh = build(&root, &store(&temp, "fresh"));
    assert_eq!(updated.snapshot_digest, fresh.snapshot_digest);
    assert_eq!(updated, fresh);
}

#[test]
fn a_file_that_crosses_the_size_limit_is_reindexed_as_skipped() {
    let temp = TempDir::new("t3a-size");
    let root = temp.path().join("repo");
    // A small limit so a normal source file crosses it.
    write(&root, "src/lib.rs", b"pub fn alpha() {}\n");
    write(&root, "src/big.rs", b"pub fn small() {}\n");
    let previous = store(&temp, "previous");
    let output = store(&temp, "output");
    let small_analyzer = analyzer_with_limit(64);
    build_snapshot(&small_analyzer, &root, &previous, BuildOptions::default()).expect("build");
    assert_eq!(
        load_manifest(&previous)
            .expect("manifest")
            .file("src/big.rs")
            .expect("big")
            .analysis_status,
        "clean"
    );

    write(
        &root,
        "src/big.rs",
        format!("pub fn big() {{}}\n// {}\n", "x".repeat(200)).as_bytes(),
    );

    let outcome = update_snapshot(
        &small_analyzer,
        &root,
        &previous,
        &output,
        BuildOptions::default(),
    )
    .expect("update");
    let record = outcome.manifest.file("src/big.rs").expect("indexed");
    assert_eq!(record.analysis_status, "unsupported");
    assert!(record.source_truncated, "the read stopped at the limit");
    assert!(record
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.contains("larger than the configured maximum")));

    let fresh = build_snapshot(
        &small_analyzer,
        &root,
        &store(&temp, "fresh"),
        BuildOptions::default(),
    )
    .expect("fresh");
    assert_eq!(
        outcome.manifest.snapshot_digest,
        fresh.manifest.snapshot_digest
    );
    assert_eq!(outcome.manifest, fresh.manifest);
}

#[test]
fn an_unchanged_recovered_file_is_reused_with_its_status_preserved() {
    let temp = TempDir::new("t3a-recovered-reuse");
    let root = scaffold(&temp);
    let previous = store(&temp, "previous");
    let output = store(&temp, "output");
    build(&root, &previous);

    let (updated, stats) = update(&root, &previous, &output);
    assert_eq!(stats.reparsed_files, 0);
    assert_eq!(stats.reused_files, stats.files_total);
    let record = updated.file("src/broken.rs").expect("recovered file");
    assert_eq!(
        record.analysis_status, "recovered",
        "reuse must not turn a recovered analysis into a clean one"
    );
    assert_eq!(updated.coverage.recovered, 1);

    let fresh = build(&root, &store(&temp, "fresh"));
    assert_eq!(updated.snapshot_digest, fresh.snapshot_digest);
}

#[test]
fn a_changed_recovered_file_is_reanalyzed() {
    let temp = TempDir::new("t3a-recovered-change");
    let root = scaffold(&temp);
    let previous = store(&temp, "previous");
    let output = store(&temp, "output");
    build(&root, &previous);

    write(
        &root,
        "src/broken.rs",
        b"fn broken( {\n    let y = ;\n    let z = ;\n}\n",
    );

    let (updated, stats) = update(&root, &previous, &output);
    assert_eq!(stats.changed, 1);
    assert_eq!(stats.reparsed_files, 1);
    assert_eq!(
        updated
            .file("src/broken.rs")
            .expect("recovered")
            .analysis_status,
        "recovered"
    );

    let fresh = build(&root, &store(&temp, "fresh"));
    assert_eq!(updated.snapshot_digest, fresh.snapshot_digest);
    assert_eq!(updated, fresh);
}

#[test]
fn an_in_place_update_over_the_previous_snapshot_is_equivalent_to_a_fresh_build() {
    let temp = TempDir::new("t3a-inplace");
    let root = scaffold(&temp);
    let snapshot = store(&temp, "snap");
    build(&root, &snapshot);

    write(
        &root,
        "src/lib.rs",
        b"pub fn alpha() {}\npub fn changed() {}\n",
    );

    // previous == output: the artifact is replaced after being read.
    let (updated, stats) = update(&root, &snapshot, &snapshot);
    assert_eq!(stats.reparsed_files, 1);
    assert_eq!(stats.reused_files, stats.files_total - 1);
    repodex::repository::verify(&snapshot).expect("in-place snapshot must verify");

    let fresh = build(&root, &store(&temp, "fresh"));
    assert_eq!(updated.snapshot_digest, fresh.snapshot_digest);
    assert_eq!(updated, fresh);
}

// ---------------------------------------------------------------------------
// Previous-snapshot validation and safe reuse
// ---------------------------------------------------------------------------

#[test]
fn an_analyzer_fingerprint_mismatch_refuses_reuse_by_default() {
    let temp = TempDir::new("t3a-fingerprint");
    let root = scaffold(&temp);
    let previous = store(&temp, "previous");
    build(&root, &previous);
    mutate_manifest(&previous, |manifest| {
        manifest.analyzer_fingerprint = format!("sha256:{}", "0".repeat(64));
    });

    let error = update_snapshot(
        &analyzer(),
        &root,
        &previous,
        &store(&temp, "output"),
        BuildOptions::default(),
    )
    .expect_err("an incompatible analyzer must not be silently reused");
    assert!(
        matches!(error, SnapshotError::FingerprintMismatch { .. }),
        "expected a fingerprint mismatch, got {error}"
    );
    assert!(
        !store(&temp, "output").exists(),
        "a refused update must not publish anything"
    );
}

#[test]
fn an_allowed_fingerprint_mismatch_rebuilds_everything_and_equals_a_fresh_build() {
    let temp = TempDir::new("t3a-fingerprint-allowed");
    let root = scaffold(&temp);
    let previous = store(&temp, "previous");
    build(&root, &previous);
    mutate_manifest(&previous, |manifest| {
        manifest.analyzer_fingerprint = format!("sha256:{}", "0".repeat(64));
    });

    let outcome = update_snapshot(
        &analyzer(),
        &root,
        &previous,
        &store(&temp, "output"),
        BuildOptions {
            allow_incompatible: true,
            ..BuildOptions::default()
        },
    )
    .expect("allowed mismatch");
    assert_eq!(
        outcome.stats.reused_files, 0,
        "an incompatible fingerprint must prevent every reuse"
    );
    assert_eq!(outcome.stats.reparsed_files, outcome.stats.files_total);

    let fresh = build(&root, &store(&temp, "fresh"));
    assert_eq!(outcome.manifest.snapshot_digest, fresh.snapshot_digest);
    assert_eq!(outcome.manifest, fresh);
}

#[test]
fn a_configuration_mismatch_refuses_reuse_by_default() {
    let temp = TempDir::new("t3a-config");
    let root = scaffold(&temp);
    let previous = store(&temp, "previous");
    build(&root, &previous);
    mutate_manifest(&previous, |manifest| {
        manifest.config.max_file_size += 1;
    });

    let error = update_snapshot(
        &analyzer(),
        &root,
        &previous,
        &store(&temp, "output"),
        BuildOptions::default(),
    )
    .expect_err("an incompatible configuration must not be silently reused");
    assert!(
        matches!(error, SnapshotError::ConfigMismatch { .. }),
        "expected a configuration mismatch, got {error}"
    );
}

#[test]
fn a_corrupt_previous_snapshot_is_refused_not_silently_rebuilt() {
    let temp = TempDir::new("t3a-corrupt");
    let root = scaffold(&temp);
    let previous = store(&temp, "previous");
    build(&root, &previous);
    std::fs::write(previous.join("manifest.json"), b"{ not json").expect("corrupt");

    let error = update_snapshot(
        &analyzer(),
        &root,
        &previous,
        &store(&temp, "output"),
        BuildOptions::default(),
    )
    .expect_err("a corrupt snapshot must be refused");
    assert!(matches!(
        error,
        SnapshotError::PreviousSnapshotUnusable { .. }
    ));
    assert!(!store(&temp, "output").exists());
}

#[test]
fn a_missing_previous_snapshot_is_refused() {
    let temp = TempDir::new("t3a-missing-previous");
    let root = scaffold(&temp);
    let error = update_snapshot(
        &analyzer(),
        &root,
        &store(&temp, "does-not-exist"),
        &store(&temp, "output"),
        BuildOptions::default(),
    )
    .expect_err("a missing previous snapshot must be refused");
    assert!(matches!(
        error,
        SnapshotError::PreviousSnapshotUnusable { .. }
    ));
}

#[test]
fn a_duplicate_file_record_is_rejected() {
    let temp = TempDir::new("t3a-duplicate");
    let root = scaffold(&temp);
    let output = store(&temp, "snap");
    build(&root, &output);
    mutate_manifest(&output, |manifest| {
        // Insert the duplicate next to its original so the list stays otherwise
        // sorted and the duplicate is what is detected.
        let duplicate = manifest.files[0].clone();
        manifest.files.insert(1, duplicate);
    });

    let error = repodex::repository::verify(&output).expect_err("duplicate must be rejected");
    assert!(
        matches!(error, SnapshotError::DuplicatePath { .. }),
        "expected a duplicate path, got {error}"
    );
}

#[test]
fn an_invalid_file_record_is_rejected() {
    let temp = TempDir::new("t3a-invalid-record");
    let root = scaffold(&temp);
    let output = store(&temp, "snap");
    build(&root, &output);
    mutate_manifest(&output, |manifest| {
        manifest.files[0].content_digest = "not-a-digest".to_string();
    });

    let error = repodex::repository::verify(&output).expect_err("invalid record must be rejected");
    assert!(matches!(error, SnapshotError::InvalidManifest { .. }));
}

#[test]
fn an_unsorted_file_list_is_rejected() {
    let temp = TempDir::new("t3a-unsorted");
    let root = scaffold(&temp);
    let output = store(&temp, "snap");
    build(&root, &output);
    mutate_manifest(&output, |manifest| {
        manifest.files.reverse();
    });

    let error = repodex::repository::verify(&output).expect_err("unsorted list must be rejected");
    assert!(matches!(
        error,
        SnapshotError::UnsortedFiles | SnapshotError::DuplicatePath { .. }
    ));
}

#[test]
fn a_wrong_snapshot_digest_is_rejected() {
    let temp = TempDir::new("t3a-wrong-digest");
    let root = scaffold(&temp);
    let output = store(&temp, "snap");
    build(&root, &output);
    mutate_manifest_stale_digest(&output, |manifest| {
        manifest.snapshot_digest = format!("sha256:{}", "f".repeat(64));
    });

    let error = repodex::repository::verify(&output).expect_err("wrong digest must be rejected");
    assert!(matches!(error, SnapshotError::InvalidManifest { .. }));
}

#[test]
fn an_unsupported_manifest_version_is_rejected() {
    let temp = TempDir::new("t3a-manifest-version");
    let root = scaffold(&temp);
    let output = store(&temp, "snap");
    build(&root, &output);
    mutate_manifest(&output, |manifest| {
        manifest.manifest_version = 99;
    });

    let error = repodex::repository::verify(&output).expect_err("version must be rejected");
    assert!(matches!(
        error,
        SnapshotError::UnsupportedManifestVersion { .. }
    ));
}

#[test]
fn a_missing_file_artifact_is_rejected() {
    let temp = TempDir::new("t3a-missing-artifact");
    let root = scaffold(&temp);
    let output = store(&temp, "snap");
    build(&root, &output);
    let key = first_object_key(&output);
    std::fs::remove_file(output.join("files").join(format!("{key}.json"))).expect("remove");

    let error =
        repodex::repository::verify(&output).expect_err("missing artifact must be rejected");
    assert!(matches!(error, SnapshotError::MissingArtifact { .. }));
}

#[test]
fn a_truncated_file_artifact_is_rejected() {
    let temp = TempDir::new("t3a-truncated");
    let root = scaffold(&temp);
    let output = store(&temp, "snap");
    build(&root, &output);
    let key = first_object_key(&output);
    let path = output.join("files").join(format!("{key}.json"));
    let bytes = std::fs::read(&path).expect("read artifact");
    std::fs::write(&path, &bytes[..bytes.len() / 2]).expect("truncate");

    let error = repodex::repository::verify(&output).expect_err("truncated artifact must fail");
    assert!(matches!(error, SnapshotError::Json { .. }));
}

#[test]
fn a_tampered_file_artifact_is_rejected_by_digest() {
    let temp = TempDir::new("t3a-tampered");
    let root = scaffold(&temp);
    let output = store(&temp, "snap");
    build(&root, &output);
    let key = first_object_key(&output);
    let path = output.join("files").join(format!("{key}.json"));
    let mut analysis: repodex::FileAnalysis =
        serde_json::from_slice(&std::fs::read(&path).expect("read")).expect("parse");
    analysis.status = repodex::AnalysisStatus::Clean;
    analysis.file.byte_len += 1;
    std::fs::write(&path, serde_json::to_vec(&analysis).expect("serialize")).expect("write");

    let error = repodex::repository::verify(&output).expect_err("tampered artifact must fail");
    assert!(matches!(
        error,
        SnapshotError::ArtifactDigestMismatch { .. }
    ));
}

#[test]
fn a_successful_build_leaves_no_staging_directory_behind() {
    let temp = TempDir::new("t3a-staging");
    let root = scaffold(&temp);
    let output = store(&temp, "snap");
    build(&root, &output);
    assert!(!repodex::repository::staging_dir(&output).exists());
    assert!(!repodex::repository::artifact::backup_dir(&output).exists());
}

// ---------------------------------------------------------------------------
// Exact repository lookup
// ---------------------------------------------------------------------------

#[test]
fn exact_lookup_returns_every_written_occurrence_without_collapsing_names() {
    let temp = TempDir::new("t3a-lookup");
    let root = scaffold(&temp);
    let output = store(&temp, "snap");
    build(&root, &output);
    let index = RepositoryFactIndex::load(&output).expect("index");

    // `helper` is declared in Go, Python and PHP.
    let helpers = index.declarations_named("helper");
    assert!(
        helpers.len() >= 3,
        "equal names in different files must not be collapsed"
    );
    let mut paths: Vec<&str> = helpers
        .iter()
        .map(|hit| hit.relative_path.as_str())
        .collect();
    paths.sort_unstable();
    assert!(paths.contains(&"main.go"));
    assert!(paths.contains(&"app.py"));
    assert!(paths.contains(&"index.php"));

    let calls = index.calls_named("helper");
    assert!(calls.len() >= 3);
    assert!(calls.iter().all(|hit| hit.callee_written == "helper"));

    // Unknown names return nothing rather than a fuzzy match.
    assert!(index
        .declarations_named("definitely_not_present")
        .is_empty());
    assert!(index.calls_named("definitely_not_present").is_empty());

    // A test-bearing declaration is found by its test evidence.
    write(&root, "src/tests.rs", b"#[test]\nfn it_works() {}\n");
    let output2 = store(&temp, "snap2");
    build(&root, &output2);
    let index2 = RepositoryFactIndex::load(&output2).expect("index2");
    let tests = index2.test_declarations();
    assert!(tests.iter().any(|hit| hit.name == "it_works"));
    assert!(tests.iter().all(|hit| hit.test_evidence > 0));
}

#[test]
fn import_lookup_matches_the_written_target_exactly() {
    let temp = TempDir::new("t3a-imports");
    let root = temp.path().join("repo");
    write(
        &root,
        "src/lib.rs",
        b"use std::collections::HashMap;\n\npub fn f() {}\n",
    );
    let output = store(&temp, "snap");
    build(&root, &output);
    let index = RepositoryFactIndex::load(&output).expect("index");
    let hits = index.import_items_targeting("std::collections::HashMap");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].relative_path, "src/lib.rs");
    assert!(index
        .import_items_targeting("std::collections::BTreeMap")
        .is_empty());
}

// ---------------------------------------------------------------------------
// Statistics
// ---------------------------------------------------------------------------

#[test]
fn repository_statistics_are_available_without_loading_trees() {
    let temp = TempDir::new("t3a-stats");
    let root = scaffold(&temp);
    let output = store(&temp, "snap");
    let manifest = build(&root, &output);

    assert_eq!(
        manifest.totals.declarations,
        manifest.files.iter().map(|f| f.declarations).sum::<u64>()
    );
    assert_eq!(
        manifest.totals.call_like,
        manifest.files.iter().map(|f| f.call_like).sum::<u64>()
    );
    assert_eq!(
        manifest.totals.source_bytes,
        manifest.files.iter().map(|f| f.source_bytes).sum::<u64>()
    );
    let by_language: std::collections::BTreeSet<&str> = manifest
        .files
        .iter()
        .map(|file| file.language.as_str())
        .collect();
    assert_eq!(
        by_language,
        ["go", "php", "python", "rust"].into_iter().collect()
    );
    assert!(manifest.coverage.is_fully_analyzed());
    assert!(!manifest.coverage.is_fully_clean());
}
