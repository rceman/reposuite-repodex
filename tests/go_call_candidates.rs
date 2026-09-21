//! TASK 4C: Go package-local plain-name call candidates.
//!
//! Builds a temp Go repo -> snapshot -> links -> candidates, then asserts each
//! plain-name call's outcome: `single_candidate`/`multiple_candidates`/
//! `no_candidate`/`out_of_scope` plus the candidate locators.

mod support;

use std::collections::BTreeSet;
use std::path::Path;

use repodex::candidates::{build_candidates, read_records, CallCandidateRecord};
use repodex::links::build_links;
use repodex::repository::{build_snapshot, BuildOptions};
use support::TempDir;

/// Build snapshot+links+candidates for a temp repo of `(path, source)` files.
/// Returns (records). `go.mod` may be included to give the files a module.
fn go_records(files: &[(&str, &str)]) -> (TempDir, Vec<CallCandidateRecord>) {
    let t = TempDir::new("t4c");
    let repo = t.path().join("repo");
    for (rel, src) in files {
        let p = repo.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, src).unwrap();
    }
    let snap = t.path().join("snap");
    build_snapshot(&support::analyzer(), &repo, &snap, BuildOptions::default()).expect("snapshot");
    let links = t.path().join("links");
    build_links(&snap, Some(&repo), &links).expect("links");
    let cand = t.path().join("cand");
    build_candidates(&snap, &links, &cand).expect("candidates");
    (t, read_records(&cand).expect("records"))
}

/// All records for calls whose written callee is `name` in `path`.
fn calls_named<'a>(
    records: &'a [CallCandidateRecord],
    path: &str,
    name: &str,
) -> Vec<&'a CallCandidateRecord> {
    records
        .iter()
        .filter(|r| r.language == "go" && r.source.relative_path == path && r.written == name)
        .collect()
}

fn only_call<'a>(
    records: &'a [CallCandidateRecord],
    path: &str,
    name: &str,
) -> &'a CallCandidateRecord {
    let found = calls_named(records, path, name);
    assert_eq!(
        found.len(),
        1,
        "expected exactly one `{name}` call in {path}"
    );
    found[0]
}

fn outcome(r: &CallCandidateRecord) -> &str {
    r.outcome.as_str()
}

fn single_target(r: &CallCandidateRecord) -> String {
    let c = r
        .outcome
        .single()
        .unwrap_or_else(|| panic!("expected single_candidate, got {}", r.outcome.as_str()));
    format!("{}#{}:{}", c.relative_path, c.declaration_id, c.name)
}

fn no_reason(r: &CallCandidateRecord) -> String {
    match &r.outcome {
        repodex::candidates::CandidateOutcome::NoCandidate { reason } => reason.clone(),
        other => panic!("expected no_candidate, got {}", other.as_str()),
    }
}

// ---------------------------------------------------------------------------
// package-local candidates
// ---------------------------------------------------------------------------

#[test]
fn ordinary_same_package_function_is_single_candidate() {
    let (_t, r) = go_records(&[(
        "auth.go",
        "package auth\nfunc validate() {}\nfunc run() { validate() }\n",
    )]);
    let c = only_call(&r, "auth.go", "validate");
    assert_eq!(outcome(c), "single_candidate");
    assert!(single_target(c).ends_with(":validate"));
}

#[test]
fn package_main_same_package_function() {
    let (_t, r) = go_records(&[(
        "main.go",
        "package main\nfunc helper() {}\nfunc main() { helper() }\n",
    )]);
    let c = only_call(&r, "main.go", "helper");
    assert_eq!(outcome(c), "single_candidate");
}

#[test]
fn external_test_cannot_borrow_ordinary_package_function() {
    let (_t, r) = go_records(&[
        ("foo/a.go", "package foo\nfunc helper() {}\n"),
        (
            "foo/x_test.go",
            "package foo_test\nfunc TestX() { helper() }\n",
        ),
    ]);
    let c = only_call(&r, "foo/x_test.go", "helper");
    // `foo_test` is a distinct package — it must NOT see `foo.helper`.
    assert_eq!(outcome(c), "no_candidate");
    assert_eq!(no_reason(c), "no_package_function");
}

#[test]
fn same_package_name_in_another_directory_does_not_borrow() {
    let (_t, r) = go_records(&[
        ("a/x.go", "package dup\nfunc helper() {}\n"),
        ("b/y.go", "package dup\nfunc run() { helper() }\n"),
    ]);
    // `b/dup` calling `helper` must NOT see `a/dup`'s `helper` (different dir).
    let c = only_call(&r, "b/y.go", "helper");
    assert_eq!(outcome(c), "no_candidate");
    assert_eq!(no_reason(c), "no_package_function");
    // `a/dup`'s own call does find it.
    let (_t2, r2) = go_records(&[(
        "a/x.go",
        "package dup\nfunc helper() {}\nfunc use() { helper() }\n",
    )]);
    assert_eq!(
        outcome(only_call(&r2, "a/x.go", "helper")),
        "single_candidate"
    );
}

#[test]
fn same_package_name_in_another_module_does_not_borrow() {
    let (_t, r) = go_records(&[
        ("go.mod", "module example.com/root\n\ngo 1.22\n"),
        ("a/x.go", "package dup\nfunc helper() {}\n"),
        ("nested/go.mod", "module example.com/nested\n\ngo 1.22\n"),
        ("nested/b/y.go", "package dup\nfunc run() { helper() }\n"),
    ]);
    // nested/b is in a different module — must not borrow a/dup's helper.
    let c = only_call(&r, "nested/b/y.go", "helper");
    assert_eq!(outcome(c), "no_candidate");
}

// ---------------------------------------------------------------------------
// local-binding blockers
// ---------------------------------------------------------------------------

#[test]
fn local_short_var_blocks_package_function() {
    let (_t, r) = go_records(&[(
        "p.go",
        "package p\nfunc helper() {}\nfunc run() {\n\thelper := func() {}\n\thelper()\n}\n",
    )]);
    let c = only_call(&r, "p.go", "helper");
    assert_eq!(outcome(c), "no_candidate");
    assert_eq!(no_reason(c), "shadowed_by_local_binding");
}

#[test]
fn function_parameter_blocks() {
    let (_t, r) = go_records(&[(
        "p.go",
        "package p\nfunc helper() {}\nfunc run(helper func()) { helper() }\n",
    )]);
    assert_eq!(
        no_reason(only_call(&r, "p.go", "helper")),
        "shadowed_by_local_binding"
    );
}

#[test]
fn named_result_blocks() {
    let (_t, r) = go_records(&[(
        "p.go",
        "package p\nfunc helper() {}\nfunc run() (helper func()) { helper(); return }\n",
    )]);
    assert_eq!(
        no_reason(only_call(&r, "p.go", "helper")),
        "shadowed_by_local_binding"
    );
}

#[test]
fn method_receiver_blocks() {
    let (_t, r) = go_records(&[(
        "p.go",
        "package p\nfunc helper() {}\ntype T struct{}\nfunc (helper *T) M() { helper() }\n",
    )]);
    assert_eq!(
        no_reason(only_call(&r, "p.go", "helper")),
        "shadowed_by_local_binding"
    );
}

#[test]
fn function_literal_parameter_blocks() {
    let (_t, r) = go_records(&[(
        "p.go",
        "package p\nfunc helper() {}\nfunc run() {\n\tf := func(helper func()) { helper() }\n\t_ = f\n}\n",
    )]);
    assert_eq!(
        no_reason(only_call(&r, "p.go", "helper")),
        "shadowed_by_local_binding"
    );
}

#[test]
fn range_variable_blocks() {
    let (_t, r) = go_records(&[(
        "p.go",
        "package p\nfunc helper() {}\nfunc run() {\n\tfor helper := range x { helper() }\n}\n",
    )]);
    assert_eq!(
        no_reason(only_call(&r, "p.go", "helper")),
        "shadowed_by_local_binding"
    );
}

#[test]
fn type_switch_variable_blocks() {
    let (_t, r) = go_records(&[(
        "p.go",
        "package p\nfunc helper() {}\nfunc run(v any) {\n\tswitch helper := v.(type) {\n\tcase func():\n\t\thelper()\n\t}\n}\n",
    )]);
    assert_eq!(
        no_reason(only_call(&r, "p.go", "helper")),
        "shadowed_by_local_binding"
    );
}

#[test]
fn select_receive_variable_blocks() {
    let (_t, r) = go_records(&[(
        "p.go",
        "package p\nfunc helper() {}\nfunc run(ch chan func()) {\n\tselect {\n\tcase helper := <-ch:\n\t\thelper()\n\t}\n}\n",
    )]);
    assert_eq!(
        no_reason(only_call(&r, "p.go", "helper")),
        "shadowed_by_local_binding"
    );
}

#[test]
fn local_var_blocks() {
    let (_t, r) = go_records(&[(
        "p.go",
        "package p\nfunc helper() {}\nfunc run() {\n\tvar helper func()\n\thelper()\n}\n",
    )]);
    assert_eq!(
        no_reason(only_call(&r, "p.go", "helper")),
        "shadowed_by_local_binding"
    );
}

#[test]
fn local_const_blocks() {
    let (_t, r) = go_records(&[(
        "p.go",
        "package p\nfunc helper() {}\nfunc run() {\n\tconst helper = 1\n\thelper()\n}\n",
    )]);
    assert_eq!(
        no_reason(only_call(&r, "p.go", "helper")),
        "shadowed_by_local_binding"
    );
}

#[test]
fn local_type_blocks() {
    let (_t, r) = go_records(&[(
        "p.go",
        "package p\nfunc helper(int) {}\nfunc run() {\n\ttype helper int\n\thelper(1)\n}\n",
    )]);
    assert_eq!(
        no_reason(only_call(&r, "p.go", "helper")),
        "shadowed_by_local_binding"
    );
}

#[test]
fn type_parameter_blocks() {
    let (_t, r) = go_records(&[(
        "p.go",
        "package p\nfunc T(v any) {}\nfunc F[T any](v any) {\n\tT(v)\n}\n",
    )]);
    let c = only_call(&r, "p.go", "T");
    assert_eq!(outcome(c), "no_candidate");
    assert_eq!(no_reason(c), "shadowed_by_local_binding");
}

// ---------------------------------------------------------------------------
// visibility edge cases
// ---------------------------------------------------------------------------

#[test]
fn call_before_local_declaration_is_a_candidate() {
    // The later `helper :=` does not cover the earlier call (§14).
    let (_t, r) = go_records(&[(
        "p.go",
        "package p\nfunc helper() {}\nfunc run() {\n\thelper()\n\thelper := func() {}\n\t_ = helper\n}\n",
    )]);
    let c = calls_named(&r, "p.go", "helper");
    // Two `helper` calls/uses: the call `helper()` then `:=`. The call is the
    // first occurrence — find the call record (there is exactly one call).
    assert_eq!(outcome(only_call(&r, "p.go", "helper")), "single_candidate");
    let _ = c;
}

#[test]
fn short_var_rhs_call_is_a_candidate() {
    // `helper := helper()` — the RHS call is eligible for package `helper` (§15).
    let (_t, r) = go_records(&[(
        "p.go",
        "package p\nfunc helper() {}\nfunc run() {\n\thelper := helper()\n\t_ = helper\n}\n",
    )]);
    assert_eq!(outcome(only_call(&r, "p.go", "helper")), "single_candidate");
}

#[test]
fn nested_blocker_does_not_leak() {
    // Inner `helper :=` blocks only its block; the outer call is a candidate.
    let (_t, r) = go_records(&[(
        "p.go",
        "package p\nfunc helper() {}\nfunc run() {\n\t{\n\t\thelper := func() {}\n\t\thelper()\n\t}\n\thelper()\n}\n",
    )]);
    let calls = calls_named(&r, "p.go", "helper");
    assert_eq!(calls.len(), 2);
    // One is blocked (inner), one is a single candidate (outer).
    let outcomes: BTreeSet<_> = calls.iter().map(|c| outcome(c)).collect();
    assert!(outcomes.contains("no_candidate") && outcomes.contains("single_candidate"));
}

#[test]
fn if_init_blocker() {
    let (_t, r) = go_records(&[(
        "p.go",
        "package p\nfunc helper() func() {}\nfunc run() {\n\tif helper := mk(); helper != nil {\n\t\thelper()\n\t}\n}\n",
    )]);
    assert_eq!(
        no_reason(only_call(&r, "p.go", "helper")),
        "shadowed_by_local_binding"
    );
}

#[test]
fn for_init_blocker() {
    // `helper :=` in the for-init covers the loop body call (§23).
    let (_t, r) = go_records(&[(
        "p.go",
        "package p\nfunc helper() func() {}\nfunc run() {\n\tfor helper := mk(); cond(); next() {\n\t\thelper()\n\t}\n\thelper()\n}\n",
    )]);
    let calls = calls_named(&r, "p.go", "helper");
    assert_eq!(calls.len(), 2);
    let outcomes: BTreeSet<_> = calls.iter().map(|c| outcome(c)).collect();
    // body call blocked by the for-init binding; the post-loop call is a candidate.
    assert!(outcomes.contains("no_candidate") && outcomes.contains("single_candidate"));
}

// ---------------------------------------------------------------------------
// import blockers
// ---------------------------------------------------------------------------

#[test]
fn import_alias_blocks_package_function() {
    let (_t, r) = go_records(&[
        ("go.mod", "module example.com/m\n\ngo 1.22\n"),
        ("helper/x.go", "package helper\n"),
        (
            "p.go",
            "package p\nimport helper \"example.com/m/helper\"\nfunc helper() {}\nfunc run() { helper() }\n",
        ),
    ]);
    let c = only_call(&r, "p.go", "helper");
    assert_eq!(outcome(c), "no_candidate");
    assert_eq!(no_reason(c), "blocked_by_import_binding");
}

#[test]
fn repo_local_normal_import_blocks_via_resolved_package_name() {
    // `import "example.com/m/helper"` — package clause is `helper`; a local
    // `func helper` exists too (inconsistent source) -> blocked.
    let (_t, r) = go_records(&[
        ("go.mod", "module example.com/m\n\ngo 1.22\n"),
        ("helper/x.go", "package helper\n"),
        (
            "p.go",
            "package p\nimport \"example.com/m/helper\"\nfunc helper() {}\nfunc run() { helper() }\n",
        ),
    ]);
    let c = only_call(&r, "p.go", "helper");
    assert_eq!(no_reason(c), "blocked_by_import_binding");
}

#[test]
fn blank_import_does_not_block() {
    let (_t, r) = go_records(&[
        ("go.mod", "module example.com/m\n\ngo 1.22\n"),
        (
            "p.go",
            "package p\nimport _ \"example.com/m/x\"\nfunc helper() {}\nfunc run() { helper() }\n",
        ),
    ]);
    assert_eq!(outcome(only_call(&r, "p.go", "helper")), "single_candidate");
}

#[test]
fn dot_import_is_namespace_uncertain() {
    let (_t, r) = go_records(&[
        ("go.mod", "module example.com/m\n\ngo 1.22\n"),
        (
            "p.go",
            "package p\nimport . \"example.com/m/x\"\nfunc helper() {}\nfunc run() { helper() }\n",
        ),
    ]);
    let c = only_call(&r, "p.go", "helper");
    assert_eq!(outcome(c), "no_candidate");
    assert_eq!(no_reason(c), "dot_import_namespace_uncertain");
}

// ---------------------------------------------------------------------------
// package namespace ambiguity / multiple functions
// ---------------------------------------------------------------------------

#[test]
fn same_name_package_type_and_function_is_ambiguous() {
    // `type helper` + `func helper` in one package (invalid/variant source).
    let (_t, r) = go_records(&[(
        "p.go",
        "package p\nfunc helper() {}\ntype helper struct{}\nfunc run() { helper() }\n",
    )]);
    let c = only_call(&r, "p.go", "helper");
    assert_eq!(outcome(c), "no_candidate");
    assert_eq!(no_reason(c), "package_namespace_ambiguous");
}

#[test]
fn same_name_package_var_and_function_is_ambiguous() {
    let (_t, r) = go_records(&[(
        "p.go",
        "package p\nfunc helper() {}\nvar helper int\nfunc run() { helper() }\n",
    )]);
    assert_eq!(
        no_reason(only_call(&r, "p.go", "helper")),
        "package_namespace_ambiguous"
    );
}

#[test]
fn multiple_package_functions_yield_multiple_candidates() {
    // Same name in two files of one package (unevaluated build tags, e.g.).
    let (_t, r) = go_records(&[
        ("p/a.go", "package p\nfunc helper() {}\n"),
        (
            "p/b.go",
            "package p\nfunc helper() {}\nfunc run() { helper() }\n",
        ),
    ]);
    let c = only_call(&r, "p/b.go", "helper");
    assert_eq!(outcome(c), "multiple_candidates");
    assert_eq!(c.outcome.candidates().len(), 2);
}

#[test]
fn unrelated_local_name_does_not_block() {
    // A local `other` binding does not block a `helper` call.
    let (_t, r) = go_records(&[(
        "p.go",
        "package p\nfunc helper() {}\nfunc run() {\n\tother := 1\n\t_ = other\n\thelper()\n}\n",
    )]);
    assert_eq!(outcome(only_call(&r, "p.go", "helper")), "single_candidate");
}

#[test]
fn no_package_function_is_no_candidate() {
    let (_t, r) = go_records(&[("p.go", "package p\nfunc run() { missing() }\n")]);
    let c = only_call(&r, "p.go", "missing");
    assert_eq!(outcome(c), "no_candidate");
    assert_eq!(no_reason(c), "no_package_function");
}

#[test]
fn member_selector_is_out_of_scope() {
    let (_t, r) = go_records(&[("p.go", "package p\nfunc run() { obj.Method() }\n")]);
    let c = only_call(&r, "p.go", "obj.Method");
    assert_eq!(outcome(c), "out_of_scope");
}

// ---------------------------------------------------------------------------
// §45 update-vs-fresh + §44 determinism + §32 stale rejection
// ---------------------------------------------------------------------------

use repodex::candidates::read_manifest as read_cand_manifest;
use repodex::repository::update_snapshot;

/// Build candidates for a repo dir, returning the candidate digest.
fn build_digest(repo: &Path, snap: &Path, links: &Path, cand: &Path) -> String {
    build_snapshot(&support::analyzer(), repo, snap, BuildOptions::default()).expect("snap");
    build_links(snap, Some(repo), links).expect("links");
    build_candidates(snap, links, cand).expect("cand");
    read_cand_manifest(cand).expect("manifest").candidate_digest
}

/// Write files into `repo`.
fn write_files(repo: &Path, files: &[(&str, &str)]) {
    for (rel, src) in files {
        let p = repo.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, src).unwrap();
    }
}

#[test]
fn determinism_repeated_build_is_byte_stable() {
    let t = TempDir::new("t4c-det");
    let repo = t.path().join("repo");
    write_files(
        &repo,
        &[(
            "p.go",
            "package p\nfunc helper() {}\nfunc run() { helper() }\n",
        )],
    );
    let a = build_digest(
        &repo,
        &t.path().join("sa"),
        &t.path().join("la"),
        &t.path().join("ca"),
    );
    let b = build_digest(
        &repo,
        &t.path().join("sb"),
        &t.path().join("lb"),
        &t.path().join("cb"),
    );
    assert_eq!(
        a, b,
        "identical bytes must produce identical candidate digest"
    );
}

#[test]
fn cross_root_determinism() {
    let t = TempDir::new("t4c-xroot");
    let files = &[(
        "sub/p.go",
        "package p\nfunc helper() {}\nfunc run() { helper() }\n",
    )];
    let ra = t.path().join("rootA");
    let rb = t.path().join("rootB");
    write_files(&ra, files);
    write_files(&rb, files);
    let da = build_digest(
        &ra,
        &t.path().join("sa"),
        &t.path().join("la"),
        &t.path().join("ca"),
    );
    let db = build_digest(
        &rb,
        &t.path().join("sb"),
        &t.path().join("lb"),
        &t.path().join("cb"),
    );
    assert_eq!(
        da, db,
        "same relative bytes at a different root must be identical"
    );
}

#[test]
fn update_vs_fresh_preserves_candidates() {
    let t = TempDir::new("t4c-uvf");
    let repo = t.path().join("repo");
    write_files(
        &repo,
        &[
            ("p/a.go", "package p\nfunc helper() {}\n"),
            ("p/run.go", "package p\nfunc run() { helper() }\n"),
        ],
    );
    let base = t.path().join("base");
    build_snapshot(&support::analyzer(), &repo, &base, BuildOptions::default()).expect("base");
    // Add a package function + a local blocker.
    write_files(&repo, &[
        ("p/extra.go", "package p\nfunc extra() {}\nfunc use() {\n\tx := 1\n\t_ = x\n\textra()\n\thelper()\n}\n"),
    ]);
    let upd_snap = t.path().join("upd-snap");
    update_snapshot(
        &support::analyzer(),
        &repo,
        &base,
        &upd_snap,
        BuildOptions::default(),
    )
    .expect("update");
    let upd_links = t.path().join("upd-links");
    build_links(&upd_snap, Some(&repo), &upd_links).expect("upd links");
    let upd_cand = t.path().join("upd-cand");
    build_candidates(&upd_snap, &upd_links, &upd_cand).expect("upd cand");
    // Fresh full build on the same bytes.
    let fresh_digest = build_digest(
        &repo,
        &t.path().join("fs"),
        &t.path().join("fl"),
        &t.path().join("fc"),
    );
    let upd_digest = read_cand_manifest(&upd_cand).unwrap().candidate_digest;
    assert_eq!(
        upd_digest, fresh_digest,
        "incremental path must equal fresh"
    );
}
