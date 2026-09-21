//! TASK 3E V2: target-aware structural qualified-path candidates.
//!
//! Unlike `structural_path_v4` (in-memory lib/main crates), these build a real
//! repository with a `Cargo.toml`, derive the TASK 3F crate/target topology,
//! and assert that `crate::`/`self::`/`super::`/relative paths resolve inside
//! the *correct* target crate — and never leak across a target boundary.

mod support;

use repodex::candidates::{candidate_rule, rule_rust, CallCandidateRecord, CandidateOutcome};
use repodex::links::{build_links, read_entities, read_links};
use repodex::repository::artifact::read_file_analysis;
use repodex::repository::{build_snapshot, BuildOptions};
use support::TempDir;

const RULE: &str = candidate_rule::RUST_CALL_STRUCTURAL_PATH_FUNCTION_CANDIDATE;

/// Build a repo, snapshot + links, then run the candidate rule against the
/// persisted target topology entities.
fn records(temp: &TempDir, files: &[(&str, &str)]) -> Vec<CallCandidateRecord> {
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
    let links_dir = temp.path().join("links");
    build_links(&snap, Some(&root), &links_dir).expect("links");
    let entities = read_entities(&links_dir).expect("entities");
    let links = read_links(&links_dir).expect("links");
    // Re-read the snapshot file analyses.
    let mut analyses = Vec::new();
    for f in &manifest.files {
        if f.language != "rust" {
            continue;
        }
        analyses.push(read_file_analysis(&snap, f).expect("analysis"));
    }
    rule_rust::candidates(&analyses, &links, &entities)
}

fn calls<'a>(records: &'a [CallCandidateRecord], written: &str) -> Vec<&'a CallCandidateRecord> {
    records
        .iter()
        .filter(|r| r.written == written && r.rule_id == RULE)
        .collect()
}

fn assert_single(records: &[CallCandidateRecord], written: &str, expected_path: &str) {
    let cs = calls(records, written);
    assert!(!cs.is_empty(), "no record for {written}");
    for r in cs {
        match &r.outcome {
            CandidateOutcome::SingleCandidate { candidate } => {
                assert_eq!(candidate.relative_path, expected_path, "{written}")
            }
            other => panic!("{written}: expected single, got {other:?}"),
        }
    }
}

fn assert_no_candidate(records: &[CallCandidateRecord], written: &str) {
    let cs = calls(records, written);
    assert!(!cs.is_empty(), "no record for {written}");
    for r in cs {
        assert!(
            matches!(
                r.outcome,
                CandidateOutcome::NoCandidate { .. } | CandidateOutcome::OutOfScope { .. }
            ),
            "{written}: expected no_candidate/out_of_scope, got {:?}",
            r.outcome
        );
    }
}

// ---------------------------------------------------------------------------
// Cross-target isolation (§9–§12)
// ---------------------------------------------------------------------------

#[test]
fn integration_test_crate_fn_resolves_in_its_own_target() {
    let t = TempDir::new("t3e2-it");
    let recs = records(
        &t,
        &[
            ("Cargo.toml", "[package]\nname = \"demo\"\n"),
            ("src/lib.rs", "pub fn helper() {}\n"),
            // The test's own crate root declares `helper`; crate::helper must
            // resolve to THIS file, not the lib's helper.
            (
                "tests/it.rs",
                "pub fn helper() {}\nfn t() { crate::helper(); }\n",
            ),
        ],
    );
    assert_single(&recs, "crate::helper", "tests/it.rs");
}

#[test]
fn integration_test_crate_fn_never_borrows_the_lib() {
    let t = TempDir::new("t3e2-itnolib");
    let recs = records(
        &t,
        &[
            ("Cargo.toml", "[package]\nname = \"demo\"\n"),
            ("src/lib.rs", "pub fn lib_only() {}\n"),
            // `crate::lib_only` in the test must NOT reach the lib target.
            ("tests/it.rs", "fn t() { crate::lib_only(); }\n"),
        ],
    );
    assert_no_candidate(&recs, "crate::lib_only");
}

#[test]
fn bin_crate_is_isolated_from_lib() {
    let t = TempDir::new("t3e2-bin");
    let recs = records(
        &t,
        &[
            ("Cargo.toml", "[package]\nname = \"app\"\n"),
            ("src/lib.rs", "pub fn lib_fn() {}\n"),
            ("src/main.rs", "fn own() {}\nfn main() { crate::own(); }\n"),
        ],
    );
    assert_single(&recs, "crate::own", "src/main.rs");
    // And a bin call for a lib-only name finds nothing.
    let t2 = TempDir::new("t3e2-bin2");
    let recs2 = records(
        &t2,
        &[
            ("Cargo.toml", "[package]\nname = \"app\"\n"),
            ("src/lib.rs", "pub fn lib_fn() {}\n"),
            ("src/main.rs", "fn main() { crate::lib_fn(); }\n"),
        ],
    );
    assert_no_candidate(&recs2, "crate::lib_fn");
}

#[test]
fn example_and_bench_are_their_own_crates() {
    let t = TempDir::new("t3e2-exb");
    let recs = records(
        &t,
        &[
            ("Cargo.toml", "[package]\nname = \"demo\"\n"),
            ("examples/ex.rs", "fn ef() {}\nfn main() { crate::ef(); }\n"),
            ("benches/b.rs", "fn bf() {}\nfn main() { crate::bf(); }\n"),
        ],
    );
    assert_single(&recs, "crate::ef", "examples/ex.rs");
    assert_single(&recs, "crate::bf", "benches/b.rs");
}

#[test]
fn two_integration_tests_are_isolated() {
    let t = TempDir::new("t3e2-twoit");
    let recs = records(
        &t,
        &[
            ("Cargo.toml", "[package]\nname = \"demo\"\n"),
            ("tests/a.rs", "fn same() {}\nfn t() { crate::same(); }\n"),
            ("tests/b.rs", "fn same() {}\nfn u() { crate::same(); }\n"),
        ],
    );
    // `crate::same` in each test resolves to that test's own file only.
    let a: Vec<_> = calls(&recs, "crate::same")
        .into_iter()
        .filter(|r| r.source.relative_path == "tests/a.rs")
        .collect();
    let b: Vec<_> = calls(&recs, "crate::same")
        .into_iter()
        .filter(|r| r.source.relative_path == "tests/b.rs")
        .collect();
    for (set, path) in [(a, "tests/a.rs"), (b, "tests/b.rs")] {
        for r in set {
            match &r.outcome {
                CandidateOutcome::SingleCandidate { candidate } => {
                    assert_eq!(candidate.relative_path, path)
                }
                other => panic!("expected single in {path}, got {other:?}"),
            }
        }
    }
}

#[test]
fn explicit_cargo_target_path_is_the_crate_root() {
    let t = TempDir::new("t3e2-explicit");
    let recs = records(
        &t,
        &[
            (
                "Cargo.toml",
                "[package]\nname = \"demo\"\n\n[[test]]\nname=\"ct\"\npath=\"custom/ct.rs\"\n",
            ),
            ("src/lib.rs", "pub fn lib_fn() {}\n"),
            ("custom/ct.rs", "fn mine() {}\nfn t() { crate::mine(); }\n"),
        ],
    );
    assert_single(&recs, "crate::mine", "custom/ct.rs");
}

// ---------------------------------------------------------------------------
// self / super / relative inside a target (§7, §14–§17)
// ---------------------------------------------------------------------------

#[test]
fn relative_module_in_integration_test_is_target_local() {
    let t = TempDir::new("t3e2-rel");
    let recs = records(
        &t,
        &[
            ("Cargo.toml", "[package]\nname = \"demo\"\n"),
            ("src/lib.rs", "pub fn f() {}\n"),
            ("tests/it.rs", "mod shared;\nfn t() { shared::helper(); }\n"),
            ("tests/shared.rs", "pub fn helper() {}\n"),
        ],
    );
    assert_single(&recs, "shared::helper", "tests/shared.rs");
}

#[test]
fn super_in_a_nested_test_module_stays_in_target() {
    let t = TempDir::new("t3e2-super");
    let recs = records(
        &t,
        &[
            ("Cargo.toml", "[package]\nname = \"demo\"\n"),
            ("src/lib.rs", "pub fn f() {}\n"),
            ("tests/it.rs", "fn root_fn() {}\nmod inner;\n"),
            ("tests/inner.rs", "fn t() { super::root_fn(); }\n"),
        ],
    );
    assert_single(&recs, "super::root_fn", "tests/it.rs");
}

#[test]
fn local_binding_shadows_relative_root_in_target() {
    let t = TempDir::new("t3e2-shadow");
    let recs = records(
        &t,
        &[
            ("Cargo.toml", "[package]\nname = \"demo\"\n"),
            ("src/lib.rs", "pub fn f() {}\n"),
            (
                "tests/it.rs",
                "mod shared;\nfn t(shared: u32) { let shared = shared; shared::helper(); }\n",
            ),
            ("tests/shared.rs", "pub fn helper() {}\n"),
        ],
    );
    // `shared` is bound by a local `let`/param → no structural candidate.
    assert_no_candidate(&recs, "shared::helper");
}

#[test]
fn import_shadows_relative_root_in_target() {
    let t = TempDir::new("t3e2-ishadow");
    let recs = records(
        &t,
        &[
            ("Cargo.toml", "[package]\nname = \"demo\"\n"),
            ("src/lib.rs", "pub fn f() {}\n"),
            (
                "tests/it.rs",
                "mod shared;\nuse std::sync::mpsc as shared;\nfn t() { shared::helper(); }\n",
            ),
            ("tests/shared.rs", "pub fn helper() {}\n"),
        ],
    );
    assert_no_candidate(&recs, "shared::helper");
}

// ---------------------------------------------------------------------------
// Workspace isolation + multiple memberships (§8, §11)
// ---------------------------------------------------------------------------

#[test]
fn two_workspace_packages_do_not_cross() {
    let t = TempDir::new("t3e2-ws");
    let recs = records(
        &t,
        &[
            ("Cargo.toml", "[workspace]\nmembers=[\"a\",\"b\"]\n"),
            ("a/Cargo.toml", "[package]\nname=\"a\"\n"),
            (
                "a/src/lib.rs",
                "mod util;\npub fn fa() { crate::util::f(); }\n",
            ),
            ("a/src/util.rs", "pub fn f() {}\n"),
            ("b/Cargo.toml", "[package]\nname=\"b\"\n"),
            (
                "b/src/lib.rs",
                "mod util;\npub fn fb() { crate::util::g(); }\n",
            ),
            ("b/src/util.rs", "pub fn g() {}\n"),
        ],
    );
    let a = calls(&recs, "crate::util::f")
        .into_iter()
        .find(|r| r.source.relative_path == "a/src/lib.rs");
    let b = calls(&recs, "crate::util::g")
        .into_iter()
        .find(|r| r.source.relative_path == "b/src/lib.rs");
    match a.map(|r| &r.outcome) {
        Some(CandidateOutcome::SingleCandidate { candidate }) => {
            assert_eq!(candidate.relative_path, "a/src/util.rs")
        }
        other => panic!("a crate::util::f: {other:?}"),
    }
    match b.map(|r| &r.outcome) {
        Some(CandidateOutcome::SingleCandidate { candidate }) => {
            assert_eq!(candidate.relative_path, "b/src/util.rs")
        }
        other => panic!("b crate::util::g: {other:?}"),
    }
}

#[test]
fn shared_support_file_evaluates_every_membership() {
    // `tests/support` is `mod`-reachable from two test targets; a `crate::`
    // path inside it must consider each crate's tree and dedup identical
    // targets — the support file's own `crate::` resolves to the support
    // module's crate (which is each test's crate). With identical trees the
    // dedup must not panic and must produce a bounded outcome.
    let t = TempDir::new("t3e2-multi");
    let recs = records(
        &t,
        &[
            ("Cargo.toml", "[package]\nname = \"demo\"\n"),
            ("src/lib.rs", "pub fn f() {}\n"),
            ("tests/a.rs", "mod support;\nfn ta() {}\n"),
            ("tests/b.rs", "mod support;\nfn tb() {}\n"),
            (
                "tests/support/mod.rs",
                "pub fn s() {}\nfn inner() { crate::s(); }\n",
            ),
        ],
    );
    // `crate::s()` in support: `crate` is the test target's root — `mod.rs`
    // files reached via `mod support` have `crate` = the *test* crate root,
    // so `crate::s` resolves to the test root's own `s`... which does not
    // exist there. The bounded outcome is no_candidate/out_of_scope, never a
    // wrong-crate target.
    assert_no_candidate(&recs, "crate::s");
}

// ---------------------------------------------------------------------------
// §37 update-vs-fresh equivalence for target-relevant changes
// ---------------------------------------------------------------------------

use repodex::candidates::artifact as cand_artifact;
use repodex::repository::update_snapshot;
use std::path::Path;

fn build_candidates(snap: &Path, root: &Path, temp: &TempDir, tag: &str) -> String {
    let links = temp.path().join(format!("{tag}-links"));
    build_links(snap, Some(root), &links).expect("links");
    let out = temp.path().join(format!("{tag}-cand"));
    repodex::candidates::build_candidates(snap, &links, &out).expect("candidates");
    cand_artifact::read_manifest(&out)
        .expect("manifest")
        .candidate_digest
}

fn write_files(root: &Path, files: &[(&str, &str)]) {
    for (rel, c) in files {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, c).unwrap();
    }
}

/// Incremental snapshot update → links → candidates must equal fresh for the
/// same final repository bytes.
fn candidate_update_equals_fresh(label: &str, files: &[(&str, &str)], mutate: impl FnOnce(&Path)) {
    let temp = TempDir::new(&format!("t3e2-eq-{label}"));
    let root = temp.path().join("repo");
    write_files(&root, files);
    let base_snap = temp.path().join("base-snap");
    build_snapshot(
        &support::analyzer(),
        &root,
        &base_snap,
        BuildOptions::default(),
    )
    .expect("base");

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
    let upd = build_candidates(&upd_snap, &root, &temp, "upd");

    let fresh_snap = temp.path().join("fresh-snap");
    build_snapshot(
        &support::analyzer(),
        &root,
        &fresh_snap,
        BuildOptions::default(),
    )
    .expect("fresh");
    let fresh = build_candidates(&fresh_snap, &root, &temp, "fresh");

    assert_eq!(
        upd, fresh,
        "{label}: update and fresh candidates must agree"
    );
}

#[test]
fn candidate_update_equals_fresh_for_integration_test_added() {
    candidate_update_equals_fresh(
        "itadd",
        &[
            ("Cargo.toml", "[package]\nname=\"d\"\n"),
            ("src/lib.rs", "pub fn f() {}\n"),
        ],
        |r| {
            write_files(
                r,
                &[
                    (
                        "Cargo.toml",
                        "[package]\nname=\"d\"\n\n[[test]]\nname=\"t\"\npath=\"tests/t.rs\"\n",
                    ),
                    ("tests/t.rs", "fn a(){}\nfn t(){crate::a();}\n"),
                ],
            );
        },
    );
}

#[test]
fn candidate_update_equals_fresh_for_integration_test_removed() {
    candidate_update_equals_fresh(
        "itrem",
        &[
            (
                "Cargo.toml",
                "[package]\nname=\"d\"\n\n[[test]]\nname=\"t\"\npath=\"tests/t.rs\"\n",
            ),
            ("src/lib.rs", "pub fn f() {}\n"),
            ("tests/t.rs", "fn a(){}\nfn t(){crate::a();}\n"),
        ],
        |r| {
            write_files(r, &[("Cargo.toml", "[package]\nname=\"d\"\n")]);
        },
    );
}

#[test]
fn candidate_update_equals_fresh_for_explicit_target_path_changed() {
    candidate_update_equals_fresh(
        "pathchg",
        &[
            (
                "Cargo.toml",
                "[package]\nname=\"d\"\n\n[[test]]\nname=\"t\"\npath=\"a/t.rs\"\n",
            ),
            ("src/lib.rs", "pub fn f() {}\n"),
            ("a/t.rs", "fn a(){}\nfn t(){crate::a();}\n"),
            ("b/t.rs", "fn a(){}\nfn t(){crate::a();}\n"),
        ],
        |r| {
            write_files(
                r,
                &[(
                    "Cargo.toml",
                    "[package]\nname=\"d\"\n\n[[test]]\nname=\"t\"\npath=\"b/t.rs\"\n",
                )],
            );
        },
    );
}

#[test]
fn candidate_update_equals_fresh_for_workspace_member_added() {
    candidate_update_equals_fresh(
        "wsadd",
        &[
            ("Cargo.toml", "[workspace]\nmembers=[\"a\"]\n"),
            ("a/Cargo.toml", "[package]\nname=\"a\"\n"),
            ("a/src/lib.rs", "mod u;\npub fn a(){crate::u::f();}\n"),
            ("a/src/u.rs", "pub fn f() {}\n"),
        ],
        |r| {
            write_files(
                r,
                &[
                    ("Cargo.toml", "[workspace]\nmembers=[\"a\",\"b\"]\n"),
                    ("b/Cargo.toml", "[package]\nname=\"b\"\n"),
                    ("b/src/lib.rs", "mod u;\npub fn b(){crate::u::g();}\n"),
                    ("b/src/u.rs", "pub fn g() {}\n"),
                ],
            );
        },
    );
}

#[test]
fn candidate_update_equals_fresh_for_function_added_in_test() {
    candidate_update_equals_fresh(
        "fnadd",
        &[
            ("Cargo.toml", "[package]\nname=\"d\"\n"),
            ("src/lib.rs", "pub fn f() {}\n"),
            ("tests/t.rs", "fn a(){}\nfn t(){crate::a();}\n"),
        ],
        |r| {
            write_files(
                r,
                &[(
                    "tests/t.rs",
                    "fn a(){}\nfn extra(){}\nfn t(){crate::a();}\n",
                )],
            );
        },
    );
}
