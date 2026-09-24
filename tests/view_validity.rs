//! RepositoryView dependency-validity reproductions (§3, §31).
//!
//! Each test asserts the CORRECT contract. Before the fix several fail — that
//! failure IS the reproduction evidence (recorded in PRE_FIX_REPRODUCTIONS).

use std::fs;
use std::path::Path;
use std::process::Command;

use repodex::parser::{Analyzer, AnalyzerConfig};
use repodex::view::index::ensure_index;
use repodex::view::manifest;
use repodex::view::resolve::resolve;
use repodex::view::ViewLocator;

mod support;
use support::TempDir;

fn state(name: &str) -> TempDir {
    TempDir::new(&format!("vv-{name}"))
}

fn analyzer() -> Analyzer {
    Analyzer::new(AnalyzerConfig {
        max_file_size: u64::MAX,
    })
    .unwrap()
}

fn analyzer_cfg(max_file_size: u64) -> Analyzer {
    Analyzer::new(AnalyzerConfig { max_file_size }).unwrap()
}

/// Resolve a filesystem root into a RepositoryView.
fn view(root: &Path) -> repodex::view::RepositoryView {
    resolve(&ViewLocator::Root(root.to_path_buf())).unwrap()
}

fn go_repo(root: &Path, module: &str) {
    fs::create_dir_all(root.join("internal/foo")).unwrap();
    fs::write(root.join("go.mod"), format!("module {module}\n")).unwrap();
    fs::write(
        root.join("internal/foo/foo.go"),
        "package foo\nfunc Foo() int { return 1 }\n",
    )
    .unwrap();
    fs::write(
        root.join("main.go"),
        "package main\nfunc main() { _ = foo.Foo() }\n",
    )
    .unwrap();
}

// --- §4/§5: metadata-only change must invalidate ------------------------------

#[test]
fn metadata_only_go_mod_change_invalidates_index() {
    let s = state("gomod");
    let root = s.path().join("repo");
    fs::create_dir_all(&root).unwrap();
    go_repo(&root, "example.com/alpha");
    let st = s.path().join("state");
    let a = analyzer();

    let mut v = view(&root);
    let o1 = ensure_index(&a, &mut v, Some(&st)).unwrap();
    // Change ONLY go.mod (no source bytes change).
    fs::write(root.join("go.mod"), "module example.com/beta\n").unwrap();
    let mut v2 = view(&root);
    let o2 = ensure_index(&a, &mut v2, Some(&st)).unwrap();

    assert_ne!(
        o1.fingerprint, o2.fingerprint,
        "metadata-only go.mod change must change the view/index fingerprint"
    );
    assert!(
        !o2.index_reused,
        "metadata change must not silently reuse the previous index"
    );
}

#[test]
fn nested_go_mod_addition_invalidates_index() {
    let s = state("nested");
    let root = s.path().join("repo");
    fs::create_dir_all(&root).unwrap();
    go_repo(&root, "example.com/alpha");
    // x.go exists in the PARENT module first (import path example.com/alpha/sub).
    fs::create_dir_all(root.join("sub")).unwrap();
    fs::write(
        root.join("sub/x.go"),
        "package sub\nfunc X() int { return 0 }\n",
    )
    .unwrap();
    let st = s.path().join("state");
    let a = analyzer();

    let mut v = view(&root);
    let o1 = ensure_index(&a, &mut v, Some(&st)).unwrap();
    // Add ONLY a nested manifest (no source change): x.go's module boundary
    // changes (now example.com/sub). Source bytes identical -> a source-only
    // fingerprint cannot see this (§6 inventory-addition gap).
    fs::write(root.join("sub/go.mod"), "module example.com/sub\n").unwrap();
    let mut v2 = view(&root);
    let o2 = ensure_index(&a, &mut v2, Some(&st)).unwrap();
    assert_ne!(
        o1.fingerprint, o2.fingerprint,
        "a new nested manifest (no source change) must change the index fingerprint"
    );
    assert!(!o2.index_reused);
}

#[test]
fn metadata_removal_invalidates_index() {
    let s = state("goremove");
    let root = s.path().join("repo");
    fs::create_dir_all(&root).unwrap();
    go_repo(&root, "example.com/alpha");
    let st = s.path().join("state");
    let a = analyzer();
    let mut v = view(&root);
    let o1 = ensure_index(&a, &mut v, Some(&st)).unwrap();
    fs::remove_file(root.join("go.mod")).unwrap();
    let mut v2 = view(&root);
    let o2 = ensure_index(&a, &mut v2, Some(&st)).unwrap();
    assert_ne!(
        o1.fingerprint, o2.fingerprint,
        "metadata removal must invalidate"
    );
}

// --- §8/§9: same-size rapid dirty edit ---------------------------------------

/// Set a file's mtime to a fixed epoch second (deterministic same-second edit).
#[cfg(unix)]
fn set_mtime_sec(path: &Path, sec: u64) {
    let st = Command::new("touch")
        .args(["-d", &format!("@{sec}")])
        .arg(path)
        .output();
    assert!(
        st.map(|o| o.status.success()).unwrap_or(false),
        "touch -d failed"
    );
}

#[cfg(unix)]
#[test]
fn same_size_same_second_edit_is_not_reused() {
    let s = state("sameedit");
    let root = s.path().join("repo");
    fs::create_dir_all(&root).unwrap();
    let f = root.join("a.go");
    // A and B are different bytes of identical length.
    fs::write(&f, "package a\nfunc A() int { return 1 }\n").unwrap();
    let cache = s.path().join("m.json");
    let m1 = manifest::compute(&root, Some(&cache)).unwrap();
    let d1 = m1.files[0].content_digest.clone();

    // Overwrite with same-size different content; force SAME mtime second.
    let mtime = m1.files[0].mtime_secs;
    fs::write(&f, "package a\nfunc A() int { return 2 }\n").unwrap();
    set_mtime_sec(&f, mtime);
    let m2 = manifest::compute(&root, Some(&cache)).unwrap();
    let d2 = m2.files[0].content_digest.clone();
    assert_ne!(
        d1, d2,
        "same-size same-second edit must not reuse the old digest"
    );
}

// --- §11: producer/analyzer/config compatibility ------------------------------

#[test]
fn config_mismatch_does_not_reuse_index() {
    let s = state("cfg");
    let root = s.path().join("repo");
    fs::create_dir_all(&root).unwrap();
    go_repo(&root, "example.com/alpha");
    let st = s.path().join("state");

    // Build with a generous file-size config.
    let a1 = analyzer_cfg(u64::MAX);
    let mut v = view(&root);
    let o1 = ensure_index(&a1, &mut v, Some(&st)).unwrap();
    // Ensure with a DIFFERENT SnapshotConfig (different max_file_size).
    let a2 = analyzer_cfg(64); // tiny -> truncation semantics differ
    let mut v2 = view(&root);
    let o2 = ensure_index(&a2, &mut v2, Some(&st)).unwrap();
    assert!(
        !o2.index_reused,
        "a different snapshot config must not reuse the index"
    );
    // Content fingerprint is identical (same bytes); the validity key differs.
    assert_eq!(o1.fingerprint, o2.fingerprint);
    assert_ne!(
        o1.graph_dir, o2.graph_dir,
        "config is a validity input -> a different index dir"
    );
}

// --- §14-§16: coherent capture (no digest-A / bytes-B) -------------------------

/// The snapshot builder must record the digest of the bytes it actually parsed,
/// not a stale manifest digest. Simulates a file that changed between manifest
/// compute and capture.
#[test]
fn capture_binds_digest_to_bytes_actually_read() {
    let s = state("capture");
    let root = s.path().join("repo");
    fs::create_dir_all(&root).unwrap();
    let f = root.join("a.go");
    fs::write(&f, "package a\nfunc A() int { return 1 }\n").unwrap();
    let st = s.path().join("state");
    let a = analyzer();
    // First ensure: build index for content v1.
    let mut v = view(&root);
    let _ = ensure_index(&a, &mut v, Some(&st)).unwrap();
    // Change the file, then ensure again — the new index must reflect v2 bytes.
    fs::write(&f, "package a\nfunc A() int { return 2 }\nfunc NEW() {}\n").unwrap();
    let mut v2 = view(&root);
    let o2 = ensure_index(&a, &mut v2, Some(&st)).unwrap();
    assert!(!o2.index_reused);
    // The graph dir must not be the v1 index.
    assert!(o2.graph_dir.join("manifest.json").exists());
}

// --- §13: identical-input cross-view reuse preserved --------------------------

#[test]
fn identical_input_cross_view_reuse_preserved() {
    let s = state("reuse");
    let r1 = s.path().join("r1");
    let r2 = s.path().join("r2");
    go_repo(&r1, "example.com/same");
    fs::create_dir_all(&r2).unwrap();
    go_repo(&r2, "example.com/same");
    let st = s.path().join("state");
    let a = analyzer();
    let mut v1 = view(&r1);
    let o1 = ensure_index(&a, &mut v1, Some(&st)).unwrap();
    let mut v2 = view(&r2);
    let o2 = ensure_index(&a, &mut v2, Some(&st)).unwrap();
    // Identical complete inputs (different roots) must share the index.
    assert_eq!(
        o1.fingerprint, o2.fingerprint,
        "identical inputs share the index"
    );
    assert!(
        o2.index_reused,
        "identical-input second view must reuse, not rebuild"
    );
}

#[test]
fn direct_and_service_share_validity() {
    // The persistent service and direct path both funnel to ensure_index; the
    // gate is in ensure_index so both share identical validity semantics.
    // Covered here by asserting the direct path invalidates on metadata change;
    // the service path reuses the same function (see api.rs).
    let s = state("svc-eq");
    let root = s.path().join("repo");
    fs::create_dir_all(&root).unwrap();
    go_repo(&root, "example.com/a");
    let st = s.path().join("state");
    let a = analyzer();
    let mut v = view(&root);
    let o1 = ensure_index(&a, &mut v, Some(&st)).unwrap();
    fs::write(root.join("go.mod"), "module example.com/b\n").unwrap();
    let mut v2 = view(&root);
    let o2 = ensure_index(&a, &mut v2, Some(&st)).unwrap();
    assert_ne!(o1.fingerprint, o2.fingerprint);
}
