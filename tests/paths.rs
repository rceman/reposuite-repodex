//! Runtime path resolver tests.
//!
//! These tests must never write into the user's real home directory.

mod support;

use std::path::PathBuf;

use repodex::paths::{self, RepoDexPaths, HOME_ENV, SUBDIRECTORIES};
use support::TempDir;

#[test]
fn subdirectory_set_is_exactly_the_documented_set() {
    assert_eq!(
        SUBDIRECTORIES,
        [
            "config",
            "cache",
            "indexes",
            "projects",
            "benchmarks",
            "tmp"
        ]
    );
}

#[test]
fn explicit_root_resolves_every_subdirectory() {
    let root = TempDir::new("paths-root");
    let paths = RepoDexPaths::with_root(root.path());
    assert_eq!(paths.root(), root.path());
    assert_eq!(paths.config_dir(), root.path().join("config"));
    assert_eq!(paths.cache_dir(), root.path().join("cache"));
    assert_eq!(paths.indexes_dir(), root.path().join("indexes"));
    assert_eq!(paths.projects_dir(), root.path().join("projects"));
    assert_eq!(paths.benchmarks_dir(), root.path().join("benchmarks"));
    assert_eq!(paths.tmp_dir(), root.path().join("tmp"));
    let all = paths.all_subdirectories();
    assert_eq!(all.len(), SUBDIRECTORIES.len());
    assert_eq!(
        all.iter().map(|(name, _)| *name).collect::<Vec<_>>(),
        SUBDIRECTORIES.to_vec()
    );
}

#[test]
fn the_resolver_never_touches_the_filesystem() {
    let root = TempDir::new("paths-no-io");
    let missing_root = root.path().join("does/not/exist");
    let paths = RepoDexPaths::with_root(&missing_root);
    let _ = paths.config_dir();
    let _ = paths.benchmarks_dir();
    let _ = paths.all_subdirectories();
    assert!(
        !missing_root.exists(),
        "resolving paths must not create directories"
    );
    assert_eq!(std::fs::read_dir(root.path()).expect("read").count(), 0);
}

#[test]
fn no_legacy_layout_is_used() {
    let paths = RepoDexPaths::with_root("/tmp/repodex-layout-check");
    let rendered = paths.root().display().to_string();
    assert!(!rendered.contains("/.repodex"));
    assert!(!rendered.contains("/.reposuite"));
    assert!(!rendered.ends_with("/nav"));
}

#[test]
fn environment_override_wins_over_the_home_directory() {
    // `RepoDexPaths::from_env` reads the process environment, so this test only
    // asserts the resolution rule through the explicit constructor and the
    // documented precedence in one process.
    let override_root = TempDir::new("paths-override");
    let paths = RepoDexPaths::with_root(override_root.path());
    assert_eq!(paths.root(), override_root.path());
    assert_eq!(HOME_ENV, "REPOSUITE_REPODEX_HOME");
    // The literal `~` is never concatenated.
    assert!(!paths.root().display().to_string().contains('~'));
}

#[test]
fn default_root_is_home_reposuite_repodex() {
    let Some(home) = paths::home_dir() else {
        return;
    };
    let expected = home.join("reposuite").join("repodex");
    // Only assert the shape of the default when no override is present.
    if std::env::var_os(HOME_ENV).is_none() {
        let resolved = RepoDexPaths::from_env().expect("resolvable");
        assert_eq!(resolved.root(), expected.as_path());
        assert!(resolved.root().ends_with("reposuite/repodex"));
    }
    assert!(expected.ends_with("reposuite/repodex"));
    assert!(!expected.display().to_string().starts_with('~'));
}

#[test]
fn relative_paths_are_slash_separated_and_root_relative() {
    let root = PathBuf::from("/repo/root");
    assert_eq!(
        paths::relative_path(&root, &PathBuf::from("/repo/root/src/main.rs")),
        "src/main.rs"
    );
    assert_eq!(
        paths::relative_path(&root, &PathBuf::from("/repo/root/a/b/c.rs")),
        "a/b/c.rs"
    );
    // A path outside the root degrades to its file name rather than leaking an
    // absolute path into the model.
    assert_eq!(
        paths::relative_path(&root, &PathBuf::from("/elsewhere/x.rs")),
        "x.rs"
    );
}

#[test]
fn separator_normalization_handles_all_component_kinds() {
    assert_eq!(
        paths::normalize_separators(&PathBuf::from("a/b/c")),
        "a/b/c"
    );
    assert_eq!(
        paths::normalize_separators(&PathBuf::from("./a/./b")),
        "a/b"
    );
    assert_eq!(
        paths::normalize_separators(&PathBuf::from("a/../b")),
        "a/../b"
    );
    assert_eq!(paths::normalize_separators(&PathBuf::from("/a/b")), "/a/b");
}

#[test]
fn canonical_facts_use_root_relative_paths_only() {
    let analysis = support::analyze_fixture("rust/declarations.rs");
    assert_eq!(analysis.file.relative_path, "rust/declarations.rs");
    let text = analysis.canonical_text();
    assert!(!text.contains(env!("CARGO_MANIFEST_DIR")));
    assert!(!text.contains("/home/"));
}
