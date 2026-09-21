//! TASK 4A: Go package & module topology fixtures.
//!
//! Build a real repository (one or more `go.mod` + Go sources), derive the
//! `GoPackageTopology`, and assert module/package identity, package kinds,
//! file roles, module ownership, nested-module isolation, and
//! external-test/command separation.

mod support;

use repodex::links::go_topology::{GoPackageKind, GoPackageTopology};
use repodex::links::structure::Structure;
use repodex::repository::artifact::read_file_analysis;
use repodex::repository::{build_snapshot, BuildOptions};
use support::TempDir;

/// Build a repo, snapshot it, and derive the Go topology.
fn topology(temp: &TempDir, files: &[(&str, &str)]) -> GoPackageTopology {
    let root = temp.path().join("repo");
    for (rel, contents) in files {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).expect("mkdir");
        std::fs::write(p, contents).expect("write");
    }
    let snap = temp.path().join("snap");
    let manifest = build_snapshot(&support::analyzer(), &root, &snap, BuildOptions::default())
        .expect("snapshot")
        .manifest;
    let mut analyses = Vec::new();
    for f in &manifest.files {
        if f.language == "go" {
            analyses.push(read_file_analysis(&snap, f).expect("analysis"));
        }
    }
    let structure = Structure::build(&analyses);
    GoPackageTopology::build(&structure, &analyses, Some(&root))
}

fn package<'a>(
    topo: &'a GoPackageTopology,
    dir: &str,
    name: &str,
) -> Option<&'a repodex::links::go_topology::GoPackage> {
    topo.packages
        .iter()
        .find(|p| p.directory == dir && p.name == name)
}

#[test]
fn single_module_root_package() {
    let t = TempDir::new("t4a-root");
    let topo = topology(
        &t,
        &[
            ("go.mod", "module example.com/demo\n"),
            ("a.go", "package demo\nfunc F() {}\n"),
        ],
    );
    assert_eq!(topo.modules.len(), 1);
    let p = package(&topo, "", "demo").expect("root package");
    assert_eq!(p.kind, GoPackageKind::Ordinary);
    assert_eq!(p.module_id.as_deref(), Some("<root>"));
    assert_eq!(p.import_path.as_deref(), Some("example.com/demo"));
    assert_eq!(topo.memberships["a.go"], p.package_id);
}

#[test]
fn nested_ordinary_package_derives_import_path() {
    let t = TempDir::new("t4a-nested");
    let topo = topology(
        &t,
        &[
            ("go.mod", "module example.com/demo\n"),
            ("internal/auth/a.go", "package auth\nfunc F() {}\n"),
        ],
    );
    let p = package(&topo, "internal/auth", "auth").expect("nested");
    assert_eq!(p.kind, GoPackageKind::Ordinary);
    assert_eq!(
        p.import_path.as_deref(),
        Some("example.com/demo/internal/auth")
    );
}

#[test]
fn package_main_is_command_main() {
    let t = TempDir::new("t4a-main");
    let topo = topology(
        &t,
        &[
            ("go.mod", "module example.com/demo\n"),
            ("cmd/tool/main.go", "package main\nfunc main() {}\n"),
        ],
    );
    let p = package(&topo, "cmd/tool", "main").expect("command");
    assert_eq!(p.kind, GoPackageKind::CommandMain);
    assert_eq!(p.import_path.as_deref(), Some("example.com/demo/cmd/tool"));
}

#[test]
fn test_file_and_external_test_package_are_distinct() {
    let t = TempDir::new("t4a-ext");
    let topo = topology(
        &t,
        &[
            ("go.mod", "module example.com/demo\n"),
            ("pkg/a.go", "package foo\n"),
            ("pkg/a_test.go", "package foo\n"),
            ("pkg/ext_test.go", "package foo_test\n"),
        ],
    );
    // `foo` (ordinary) and `foo_test` (external_test) are distinct packages.
    let foo = package(&topo, "pkg", "foo").expect("foo");
    let foo_test = package(&topo, "pkg", "foo_test").expect("foo_test");
    assert_eq!(foo.kind, GoPackageKind::Ordinary);
    assert_eq!(foo_test.kind, GoPackageKind::ExternalTest);
    assert_ne!(foo.package_id, foo_test.package_id);
    // Ordinary package holds the source + in-package test file.
    assert_eq!(foo.files.len(), 2);
    // External-test package gets no fabricated canonical import path.
    assert_eq!(foo_test.import_path, None);
    assert_eq!(topo.memberships["pkg/ext_test.go"], foo_test.package_id);
}

#[test]
fn two_directories_with_same_package_name_are_distinct() {
    let t = TempDir::new("t4a-samename");
    let topo = topology(
        &t,
        &[
            ("go.mod", "module example.com/demo\n"),
            ("a/util.go", "package util\n"),
            ("b/util.go", "package util\n"),
        ],
    );
    let a = package(&topo, "a", "util").expect("a");
    let b = package(&topo, "b", "util").expect("b");
    assert_ne!(a.package_id, b.package_id);
    assert_eq!(a.import_path.as_deref(), Some("example.com/demo/a"));
    assert_eq!(b.import_path.as_deref(), Some("example.com/demo/b"));
}

#[test]
fn a_directory_with_conflicting_package_names_keeps_both() {
    let t = TempDir::new("t4a-conflict");
    let topo = topology(
        &t,
        &[
            ("go.mod", "module example.com/demo\n"),
            ("dir/a.go", "package alpha\n"),
            ("dir/b.go", "package beta\n"),
        ],
    );
    // Both declared packages are preserved — never repaired (§9).
    assert!(package(&topo, "dir", "alpha").is_some());
    assert!(package(&topo, "dir", "beta").is_some());
    assert!(topo
        .diagnostics
        .iter()
        .any(|d| d.contains("multiple package names")));
}

#[test]
fn a_file_outside_any_module_is_no_known_module() {
    let t = TempDir::new("t4a-nomod");
    let topo = topology(
        &t,
        &[
            // No go.mod at all.
            ("standalone/x.go", "package solo\n"),
        ],
    );
    assert!(topo.modules.is_empty());
    let p = package(&topo, "standalone", "solo").expect("package");
    assert_eq!(p.module_id, None);
    assert_eq!(p.import_path, None);
    assert_eq!(topo.memberships["standalone/x.go"], p.package_id);
}

#[test]
fn nested_go_mod_is_a_distinct_module_boundary() {
    let t = TempDir::new("t4a-nested-mod");
    let topo = topology(
        &t,
        &[
            ("go.mod", "module example.com/root\n"),
            ("pkg/a.go", "package pkg\n"),
            ("nested/go.mod", "module example.com/nested\n"),
            ("nested/pkg/b.go", "package pkg\n"),
        ],
    );
    assert_eq!(topo.modules.len(), 2);
    let outer = package(&topo, "pkg", "pkg").expect("outer pkg");
    let inner = package(&topo, "nested/pkg", "pkg").expect("inner pkg");
    assert_eq!(outer.module_id.as_deref(), Some("<root>"));
    assert_eq!(inner.module_id.as_deref(), Some("nested"));
    assert_eq!(outer.import_path.as_deref(), Some("example.com/root/pkg"));
    assert_eq!(inner.import_path.as_deref(), Some("example.com/nested/pkg"));
    // Same package name + same leaf dir, but different modules -> distinct.
    assert_ne!(outer.package_id, inner.package_id);
}

#[test]
fn two_sibling_modules_do_not_cross() {
    let t = TempDir::new("t4a-siblings");
    let topo = topology(
        &t,
        &[
            ("a/go.mod", "module example.com/a\n"),
            ("a/internal/auth/x.go", "package auth\n"),
            ("b/go.mod", "module example.com/b\n"),
            ("b/internal/auth/y.go", "package auth\n"),
        ],
    );
    let a = package(&topo, "a/internal/auth", "auth").expect("a auth");
    let b = package(&topo, "b/internal/auth", "auth").expect("b auth");
    assert_eq!(a.module_id.as_deref(), Some("a"));
    assert_eq!(b.module_id.as_deref(), Some("b"));
    assert_eq!(
        a.import_path.as_deref(),
        Some("example.com/a/internal/auth")
    );
    assert_eq!(
        b.import_path.as_deref(),
        Some("example.com/b/internal/auth")
    );
    assert_ne!(a.package_id, b.package_id);
    assert_eq!(a.files, vec!["a/internal/auth/x.go"]);
    assert_eq!(b.files, vec!["b/internal/auth/y.go"]);
}

#[test]
fn malformed_go_mod_bounds_its_subtree_as_no_known_module() {
    let t = TempDir::new("t4a-malformed");
    let topo = topology(
        &t,
        &[
            ("go.mod", "module example.com/root\n"),
            ("pkg/a.go", "package pkg\n"),
            // A go.mod with no `module` directive still bounds the subtree —
            // the files are NoKnownModule, not root members.
            ("nested/go.mod", "go 1.21\nrequire x v1\n"),
            ("nested/p.go", "package p\n"),
        ],
    );
    assert_eq!(topo.modules.len(), 1); // only the valid root module
    let p = package(&topo, "nested", "p").expect("nested pkg");
    assert_eq!(p.module_id, None);
    assert_eq!(p.import_path, None);
}

#[test]
fn build_constraint_variants_are_preserved_not_chosen() {
    // §31: `//go:build` is not evaluated. Two files that would build-tag into
    // different package names stay as two structural package variants.
    let t = TempDir::new("t4a-buildtags");
    let topo = topology(
        &t,
        &[
            ("go.mod", "module example.com/demo\n"),
            ("dir/x_linux.go", "//go:build linux\npackage alpha\n"),
            ("dir/x_windows.go", "//go:build windows\npackage beta\n"),
        ],
    );
    assert!(package(&topo, "dir", "alpha").is_some());
    assert!(package(&topo, "dir", "beta").is_some());
    assert!(topo
        .diagnostics
        .iter()
        .any(|d| d.contains("multiple package names")));
}

#[test]
fn topology_is_deterministic_for_identical_bytes() {
    let files = &[
        ("go.mod", "module example.com/demo\n"),
        ("a/util.go", "package util\n"),
        ("nested/go.mod", "module example.com/n\n"),
        ("nested/x.go", "package x\n"),
    ];
    let t1 = TempDir::new("t4a-det1");
    let t2 = TempDir::new("t4a-det2");
    let a = topology(&t1, files);
    let b = topology(&t2, files);
    let ids = |t: &GoPackageTopology| {
        t.packages
            .iter()
            .map(|p| p.package_id.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(ids(&a), ids(&b));
}

// ---------------------------------------------------------------------------
// §46 update-vs-fresh equivalence for module/config changes
// ---------------------------------------------------------------------------

use repodex::links::{build_links, read_manifest};
use repodex::repository::update_snapshot;
use std::path::Path as FsPath;

fn link_digest(snap: &FsPath, root: &FsPath, temp: &TempDir, tag: &str) -> String {
    let out = temp.path().join(format!("{tag}-links"));
    build_links(snap, Some(root), &out).expect("links");
    read_manifest(&out).expect("manifest").link_digest
}

fn write_files(root: &FsPath, files: &[(&str, &str)]) {
    for (rel, c) in files {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, c).unwrap();
    }
}

fn update_equals_fresh(label: &str, files: &[(&str, &str)], mutate: impl FnOnce(&FsPath)) {
    let temp = TempDir::new(&format!("t4a-eq-{label}"));
    let root = temp.path().join("repo");
    write_files(&root, files);
    let base = temp.path().join("base");
    build_snapshot(&support::analyzer(), &root, &base, BuildOptions::default()).expect("base");
    mutate(&root);
    let upd = temp.path().join("upd");
    update_snapshot(
        &support::analyzer(),
        &root,
        &base,
        &upd,
        BuildOptions::default(),
    )
    .expect("update");
    let upd_digest = link_digest(&upd, &root, &temp, "upd");
    let fresh = temp.path().join("fresh");
    build_snapshot(&support::analyzer(), &root, &fresh, BuildOptions::default()).expect("fresh");
    let fresh_digest = link_digest(&fresh, &root, &temp, "fresh");
    assert_eq!(upd_digest, fresh_digest, "{label}");
}

#[test]
fn update_equals_fresh_when_go_mod_added() {
    update_equals_fresh("modadd", &[("a.go", "package demo\nfunc F(){}\n")], |r| {
        write_files(r, &[("go.mod", "module example.com/d\n")])
    });
}

#[test]
fn update_equals_fresh_when_go_mod_removed() {
    update_equals_fresh(
        "modrm",
        &[
            ("go.mod", "module example.com/d\n"),
            ("a.go", "package demo\n"),
        ],
        |r| std::fs::remove_file(r.join("go.mod")).unwrap(),
    );
}

#[test]
fn update_equals_fresh_when_module_path_changes() {
    update_equals_fresh(
        "modpath",
        &[
            ("go.mod", "module example.com/a\n"),
            ("p/x.go", "package p\n"),
        ],
        |r| write_files(r, &[("go.mod", "module example.com/b\n")]),
    );
}

#[test]
fn update_equals_fresh_when_nested_module_added() {
    update_equals_fresh(
        "nestadd",
        &[
            ("go.mod", "module example.com/r\n"),
            ("nested/p.go", "package p\n"),
        ],
        |r| write_files(r, &[("nested/go.mod", "module example.com/n\n")]),
    );
}

#[test]
fn update_equals_fresh_when_external_test_added() {
    update_equals_fresh(
        "extadd",
        &[
            ("go.mod", "module example.com/d\n"),
            ("p/a.go", "package p\n"),
        ],
        |r| write_files(r, &[("p/a_test.go", "package p_test\n")]),
    );
}
