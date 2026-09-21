//! TASK 3F: Rust crate/target topology from Cargo manifests.
//!
//! Each test builds a small repository (a `Cargo.toml` plus `.rs` sources),
//! runs the snapshot + link pipeline, and asserts the `rust_crate_target`
//! structural entities — the crate roots and their member files.

mod support;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use repodex::links::{build_links, read_entities, StructuralEntity};
use repodex::repository::{build_snapshot, BuildOptions};
use support::TempDir;

fn repo(temp: &TempDir, files: &[(&str, &str)]) -> PathBuf {
    let root = temp.path().join("repo");
    for (relative, contents) in files {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).expect("mkdir");
        std::fs::write(path, contents).expect("write");
    }
    root
}

fn topology(temp: &TempDir, files: &[(&str, &str)]) -> Vec<StructuralEntity> {
    let root = repo(temp, files);
    let snap = temp.path().join("snap");
    build_snapshot(&support::analyzer(), &root, &snap, BuildOptions::default()).expect("snapshot");
    let links = temp.path().join("links");
    build_links(&snap, Some(&root), &links).expect("links");
    read_entities(&links)
        .expect("entities")
        .into_iter()
        .filter(|e| e.structural_kind == "rust_crate_target")
        .collect()
}

fn field<'a>(entity: &'a StructuralEntity, key: &str) -> &'a str {
    entity
        .assumptions
        .iter()
        .find_map(|a| a.strip_prefix(&format!("{key}=")))
        .unwrap_or("")
        .trim()
}

/// `(kind, name, root_file)` for every discovered target.
fn targets(entities: &[StructuralEntity]) -> BTreeSet<(String, String, String)> {
    entities
        .iter()
        .map(|e| {
            (
                field(e, "target_kind").to_string(),
                field(e, "target_name").to_string(),
                field(e, "root_file").to_string(),
            )
        })
        .collect()
}

fn member_targets(entities: &[StructuralEntity], file: &str) -> Vec<String> {
    let mut set = BTreeSet::new();
    for e in entities {
        if e.files.iter().any(|f| f == file) {
            set.insert(e.key.clone());
        }
    }
    set.into_iter().collect()
}

// ---------------------------------------------------------------------------
// Single-package fixtures (§26)
// ---------------------------------------------------------------------------

#[test]
fn default_src_lib_is_a_lib_target() {
    let t = TempDir::new("t3f-lib");
    let entities = topology(
        &t,
        &[
            ("Cargo.toml", "[package]\nname = \"demo\"\n"),
            ("src/lib.rs", "pub fn helper() {}\n"),
        ],
    );
    assert_eq!(
        targets(&entities),
        BTreeSet::from([(
            "lib".to_string(),
            "demo".to_string(),
            "src/lib.rs".to_string()
        )])
    );
    assert_eq!(
        member_targets(&entities, "src/lib.rs"),
        vec!["::lib::demo".to_string()]
    );
}

#[test]
fn default_src_main_is_a_bin_target() {
    let t = TempDir::new("t3f-main");
    let entities = topology(
        &t,
        &[
            ("Cargo.toml", "[package]\nname = \"app\"\n"),
            ("src/main.rs", "fn main() {}\n"),
        ],
    );
    assert_eq!(
        targets(&entities),
        BTreeSet::from([("bin".to_string(), "".to_string(), "src/main.rs".to_string())])
    );
}

#[test]
fn lib_plus_bin_are_separate_crates() {
    let t = TempDir::new("t3f-libbin");
    let entities = topology(
        &t,
        &[
            ("Cargo.toml", "[package]\nname = \"app\"\n"),
            ("src/lib.rs", "pub fn lib_fn() {}\n"),
            ("src/main.rs", "fn main() { crate::own(); }\nfn own() {}\n"),
        ],
    );
    let kinds: BTreeSet<String> = entities
        .iter()
        .map(|e| field(e, "target_kind").to_string())
        .collect();
    assert!(kinds.contains("lib"));
    assert!(kinds.contains("bin"));
    // The lib target's tree must not contain main.rs and vice versa.
    let lib = entities
        .iter()
        .find(|e| field(e, "target_kind") == "lib")
        .unwrap();
    assert!(lib.files.contains(&"src/lib.rs".to_string()));
    assert!(!lib.files.contains(&"src/main.rs".to_string()));
}

#[test]
fn explicit_lib_path_wins() {
    let t = TempDir::new("t3f-libpath");
    let entities = topology(
        &t,
        &[
            (
                "Cargo.toml",
                "[package]\nname = \"demo\"\n\n[lib]\npath = \"core/lib.rs\"\n",
            ),
            ("core/lib.rs", "pub fn f() {}\n"),
        ],
    );
    assert_eq!(
        targets(&entities),
        BTreeSet::from([(
            "lib".to_string(),
            "demo".to_string(),
            "core/lib.rs".to_string()
        )])
    );
    assert_eq!(
        field(
            entities
                .iter()
                .find(|e| field(e, "target_kind") == "lib")
                .unwrap(),
            "rule"
        ),
        "rust.target.lib.explicit_path"
    );
}

#[test]
fn explicit_bin_path_wins() {
    let t = TempDir::new("t3f-binpath");
    let entities = topology(
        &t,
        &[
            (
                "Cargo.toml",
                "[package]\nname = \"app\"\n\n[[bin]]\nname = \"srv\"\npath = \"cmd/server.rs\"\n",
            ),
            ("cmd/server.rs", "fn main() {}\n"),
        ],
    );
    assert_eq!(
        targets(&entities),
        BTreeSet::from([(
            "bin".to_string(),
            "srv".to_string(),
            "cmd/server.rs".to_string()
        )])
    );
}

#[test]
fn src_bin_file_and_dir_are_bins() {
    let t = TempDir::new("t3f-srcbin");
    let entities = topology(
        &t,
        &[
            ("Cargo.toml", "[package]\nname = \"app\"\n"),
            ("src/bin/tool.rs", "fn main() {}\n"),
            ("src/bin/multi/main.rs", "fn main() {}\n"),
        ],
    );
    let names: BTreeSet<String> = entities
        .iter()
        .map(|e| field(e, "target_name").to_string())
        .collect();
    assert!(names.contains("tool"), "{names:?}");
    assert!(names.contains("multi"), "{names:?}");
}

#[test]
fn integration_test_is_a_separate_crate() {
    let t = TempDir::new("t3f-itest");
    let entities = topology(
        &t,
        &[
            ("Cargo.toml", "[package]\nname = \"demo\"\n"),
            ("src/lib.rs", "pub fn helper() {}\n"),
            ("tests/it.rs", "mod shared;\nfn t() {}\n"),
            ("tests/shared/mod.rs", "pub fn s() {}\n"),
        ],
    );
    let it = entities
        .iter()
        .find(|e| field(e, "root_file") == "tests/it.rs")
        .expect("it target");
    assert_eq!(field(it, "target_kind"), "integration_test");
    // `mod shared` in the test reaches tests/shared, not src/.
    assert!(it.files.contains(&"tests/shared/mod.rs".to_string()));
    assert!(!it.files.iter().any(|f| f.starts_with("src/")));
}

#[test]
fn example_and_bench_are_targets() {
    let t = TempDir::new("t3f-ex-bench");
    let entities = topology(
        &t,
        &[
            ("Cargo.toml", "[package]\nname = \"demo\"\n"),
            ("examples/ex.rs", "fn main() {}\n"),
            ("benches/b.rs", "fn main() {}\n"),
        ],
    );
    let kinds: BTreeSet<String> = entities
        .iter()
        .map(|e| field(e, "target_kind").to_string())
        .collect();
    assert!(kinds.contains("example"));
    assert!(kinds.contains("bench"));
}

#[test]
fn a_file_with_no_crate_has_no_membership() {
    let t = TempDir::new("t3f-none");
    let entities = topology(
        &t,
        &[
            ("Cargo.toml", "[package]\nname = \"demo\"\n"),
            ("src/lib.rs", "pub fn f() {}\n"),
            ("scratch.rs", "pub fn orphan() {}\n"),
        ],
    );
    assert!(member_targets(&entities, "scratch.rs").is_empty());
    assert_eq!(member_targets(&entities, "src/lib.rs").len(), 1);
}

// ---------------------------------------------------------------------------
// Workspace fixtures (§27)
// ---------------------------------------------------------------------------

#[test]
fn workspace_two_members_separate_crates() {
    let t = TempDir::new("t3f-ws");
    let entities = topology(
        &t,
        &[
            ("Cargo.toml", "[workspace]\nmembers = [\"a\", \"b\"]\n"),
            ("a/Cargo.toml", "[package]\nname = \"a\"\n"),
            ("a/src/lib.rs", "mod shared;\npub fn a_fn() {}\n"),
            ("a/src/shared.rs", "pub fn s() {}\n"),
            ("b/Cargo.toml", "[package]\nname = \"b\"\n"),
            ("b/src/lib.rs", "pub fn b_fn() {}\n"),
        ],
    );
    // Two lib targets, one per package, isolated.
    let a = entities
        .iter()
        .find(|e| field(e, "package") == "a")
        .expect("a lib");
    let b = entities
        .iter()
        .find(|e| field(e, "package") == "b")
        .expect("b lib");
    assert!(a.files.contains(&"a/src/shared.rs".to_string()));
    assert!(!a.files.iter().any(|f| f.starts_with("b/")));
    assert!(b.files.contains(&"b/src/lib.rs".to_string()));
    assert_eq!(member_targets(&entities, "a/src/shared.rs").len(), 1);
}

#[test]
fn workspace_member_glob_resolves() {
    let t = TempDir::new("t3f-wsglob");
    let entities = topology(
        &t,
        &[
            ("Cargo.toml", "[workspace]\nmembers = [\"crates/*\"]\n"),
            ("crates/x/Cargo.toml", "[package]\nname = \"x\"\n"),
            ("crates/x/src/lib.rs", "pub fn x() {}\n"),
            ("crates/y/Cargo.toml", "[package]\nname = \"y\"\n"),
            ("crates/y/src/lib.rs", "pub fn y() {}\n"),
        ],
    );
    let pkgs: BTreeSet<String> = entities
        .iter()
        .map(|e| field(e, "package").to_string())
        .collect();
    assert!(pkgs.contains("x") && pkgs.contains("y"), "{pkgs:?}");
}

#[test]
fn workspace_exclude_drops_a_member() {
    let t = TempDir::new("t3f-wsexcl");
    let entities = topology(
        &t,
        &[
            (
                "Cargo.toml",
                "[workspace]\nmembers = [\"a\", \"skip\"]\nexclude = [\"skip\"]\n",
            ),
            ("a/Cargo.toml", "[package]\nname = \"a\"\n"),
            ("a/src/lib.rs", "pub fn a() {}\n"),
            ("skip/Cargo.toml", "[package]\nname = \"skip\"\n"),
            ("skip/src/lib.rs", "pub fn s() {}\n"),
        ],
    );
    let pkgs: BTreeSet<String> = entities
        .iter()
        .map(|e| field(e, "package").to_string())
        .collect();
    assert!(pkgs.contains("a"));
    assert!(!pkgs.contains("skip"), "excluded member must not appear");
}

#[test]
fn same_module_name_in_two_packages_is_isolated() {
    let t = TempDir::new("t3f-samemod");
    let entities = topology(
        &t,
        &[
            ("Cargo.toml", "[workspace]\nmembers = [\"a\", \"b\"]\n"),
            ("a/Cargo.toml", "[package]\nname = \"a\"\n"),
            (
                "a/src/lib.rs",
                "mod util;\npub fn fa() { crate::util::f(); }\n",
            ),
            ("a/src/util.rs", "pub fn f() {}\n"),
            ("b/Cargo.toml", "[package]\nname = \"b\"\n"),
            (
                "b/src/lib.rs",
                "mod util;\npub fn fb() { crate::util::g(); }\n",
            ),
            ("b/src/util.rs", "pub fn g() {}\n"),
        ],
    );
    // Each `util` is reachable only from its own package's lib.
    assert_eq!(
        member_targets(&entities, "a/src/util.rs"),
        vec!["a::lib::a".to_string()]
    );
    assert_eq!(
        member_targets(&entities, "b/src/util.rs"),
        vec!["b::lib::b".to_string()]
    );
}

// ---------------------------------------------------------------------------
// Cross-target isolation (§18, §28)
// ---------------------------------------------------------------------------

#[test]
fn integration_test_crate_root_does_not_see_lib() {
    let t = TempDir::new("t3f-isolate");
    let entities = topology(
        &t,
        &[
            ("Cargo.toml", "[package]\nname = \"demo\"\n"),
            ("src/lib.rs", "pub fn helper() {}\n"),
            ("tests/it.rs", "fn t() { crate::helper(); }\n"),
        ],
    );
    let it = entities
        .iter()
        .find(|e| field(e, "root_file") == "tests/it.rs")
        .unwrap();
    // The test's crate contains only its own files — the lib's `helper` is a
    // different crate and is not structurally reachable.
    assert_eq!(it.files, vec!["tests/it.rs".to_string()]);
    assert!(!it.files.contains(&"src/lib.rs".to_string()));
}

#[test]
fn two_integration_tests_are_independent() {
    let t = TempDir::new("t3f-two-it");
    let entities = topology(
        &t,
        &[
            ("Cargo.toml", "[package]\nname = \"demo\"\n"),
            ("tests/one.rs", "mod shared;\nfn a() {}\n"),
            ("tests/two.rs", "mod shared;\nfn b() {}\n"),
            ("tests/shared/mod.rs", "pub fn s() {}\n"),
        ],
    );
    // `tests/shared` is a member of BOTH test crates (each `mod shared`).
    let shared = member_targets(&entities, "tests/shared/mod.rs");
    assert_eq!(shared.len(), 2);
    // But each test root is only in its own crate.
    assert_eq!(member_targets(&entities, "tests/one.rs").len(), 1);
    assert_eq!(member_targets(&entities, "tests/two.rs").len(), 1);
}

// ---------------------------------------------------------------------------
// §41 update-vs-fresh equivalence for target-relevant changes
// ---------------------------------------------------------------------------

use repodex::repository::update_snapshot;

fn link_manifest_digest(snapshot_dir: &Path, root: &Path, out: &Path) -> String {
    build_links(snapshot_dir, Some(root), out)
        .expect("link build")
        .manifest
        .link_digest
}

/// Incremental snapshot update → link rebuild must equal a fresh snapshot →
/// link build for the same final bytes.
fn topology_update_equals_fresh(label: &str, files: &[(&str, &str)], mutate: impl FnOnce(&Path)) {
    let temp = TempDir::new(&format!("t3f-eq-{label}"));
    let root = repo(&temp, files);
    let base_snap = temp.path().join("base-snap");
    build_snapshot(
        &support::analyzer(),
        &root,
        &base_snap,
        BuildOptions::default(),
    )
    .expect("base snapshot");

    mutate(&root);

    let upd_snap = temp.path().join("upd-snap");
    update_snapshot(
        &support::analyzer(),
        &root,
        &base_snap,
        &upd_snap,
        BuildOptions::default(),
    )
    .expect("update");
    let upd = link_manifest_digest(&upd_snap, &root, &temp.path().join("upd-links"));

    let fresh_snap = temp.path().join("fresh-snap");
    build_snapshot(
        &support::analyzer(),
        &root,
        &fresh_snap,
        BuildOptions::default(),
    )
    .expect("fresh snapshot");
    let fresh = link_manifest_digest(&fresh_snap, &root, &temp.path().join("fresh-links"));

    assert_eq!(
        upd, fresh,
        "{label}: update and fresh link digests must agree"
    );
}

#[test]
fn update_equals_fresh_for_a_manifest_target_added() {
    topology_update_equals_fresh(
        "tgtadd",
        &[
            ("Cargo.toml", "[package]\nname = \"demo\"\n"),
            ("src/lib.rs", "pub fn f() {}\n"),
        ],
        |root| {
            std::fs::write(
                root.join("Cargo.toml"),
                "[package]\nname = \"demo\"\n\n[[test]]\nname=\"t\"\npath=\"tests/t.rs\"\n",
            )
            .expect("write");
            std::fs::create_dir_all(root.join("tests")).unwrap();
            std::fs::write(root.join("tests/t.rs"), "fn t() {}\n").unwrap();
        },
    );
}

#[test]
fn update_equals_fresh_for_a_manifest_target_removed() {
    topology_update_equals_fresh(
        "tgtrem",
        &[
            (
                "Cargo.toml",
                "[package]\nname = \"demo\"\n\n[[test]]\nname=\"t\"\npath=\"tests/t.rs\"\n",
            ),
            ("src/lib.rs", "pub fn f() {}\n"),
            ("tests/t.rs", "fn t() {}\n"),
        ],
        |root| {
            std::fs::write(root.join("Cargo.toml"), "[package]\nname = \"demo\"\n").expect("write");
        },
    );
}

#[test]
fn update_equals_fresh_for_an_explicit_path_changed() {
    topology_update_equals_fresh(
        "pathchg",
        &[
            (
                "Cargo.toml",
                "[package]\nname = \"demo\"\n\n[lib]\npath=\"core/lib.rs\"\n",
            ),
            ("core/lib.rs", "pub fn f() {}\n"),
            ("alt/lib.rs", "pub fn f() {}\n"),
        ],
        |root| {
            std::fs::write(
                root.join("Cargo.toml"),
                "[package]\nname = \"demo\"\n\n[lib]\npath=\"alt/lib.rs\"\n",
            )
            .expect("write");
        },
    );
}

#[test]
fn update_equals_fresh_for_a_workspace_member_added() {
    topology_update_equals_fresh(
        "wsadd",
        &[
            ("Cargo.toml", "[workspace]\nmembers=[\"a\"]\n"),
            ("a/Cargo.toml", "[package]\nname=\"a\"\n"),
            ("a/src/lib.rs", "pub fn a() {}\n"),
        ],
        |root| {
            std::fs::write(
                root.join("Cargo.toml"),
                "[workspace]\nmembers=[\"a\",\"b\"]\n",
            )
            .expect("write");
            std::fs::create_dir_all(root.join("b/src")).unwrap();
            std::fs::write(root.join("b/Cargo.toml"), "[package]\nname=\"b\"\n").unwrap();
            std::fs::write(root.join("b/src/lib.rs"), "pub fn b() {}\n").unwrap();
        },
    );
}

#[test]
fn update_equals_fresh_for_a_source_under_a_known_target_changed() {
    topology_update_equals_fresh(
        "srcchg",
        &[
            ("Cargo.toml", "[package]\nname = \"demo\"\n"),
            ("src/lib.rs", "mod util;\npub fn f() {}\n"),
            ("src/util.rs", "pub fn helper() {}\n"),
        ],
        |root| {
            std::fs::write(
                root.join("src/util.rs"),
                "pub fn helper() {}\npub fn extra() {}\n",
            )
            .expect("write");
        },
    );
}
