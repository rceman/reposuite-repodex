//! TASK 3E structural qualified-path candidate fixtures.
//!
//! These exercise `rule_rust::candidates` on a multi-file crate: the rule
//! descends the persisted `Structure` module tree so a `qualified_path` call
//! only produces a `function` candidate when the whole `::` path is
//! structurally proven — never from spelling.

mod support;

use repodex::candidates::{candidate_rule, rule_rust, CallCandidateRecord, CandidateOutcome};
use repodex::model::{FileAnalysis, LanguageId};
use support::analyze_source;

const RULE: &str = candidate_rule::RUST_CALL_STRUCTURAL_PATH_FUNCTION_CANDIDATE;

/// Build the `FileAnalysis` set for a crate rooted at `src/lib.rs`.
fn analyses(files: &[(&str, &str)]) -> Vec<FileAnalysis> {
    files
        .iter()
        .map(|(path, source)| analyze_source(LanguageId::Rust, path, source.as_bytes()))
        .collect()
}

fn records(files: &[(&str, &str)]) -> Vec<CallCandidateRecord> {
    rule_rust::candidates(&analyses(files), &[], &[])
}

/// All `qualified_path` records whose written callee is `written`.
fn calls_to<'a>(records: &'a [CallCandidateRecord], written: &str) -> Vec<&'a CallCandidateRecord> {
    records.iter().filter(|r| r.written == written).collect()
}

fn evidence(record: &CallCandidateRecord) -> String {
    record.provenance.evidence.join(";")
}

/// Assert `written` produced a `single_candidate`/`multiple_candidates` naming
/// `expected` (path#decl `name` list) under the structural-path rule.
fn assert_candidates(records: &[CallCandidateRecord], written: &str, expected: &[&str]) {
    let calls = calls_to(records, written);
    assert!(!calls.is_empty(), "no record for {written}");
    for record in &calls {
        assert_eq!(record.rule_id, RULE, "{written} rule");
        let names: Vec<&str> = match &record.outcome {
            CandidateOutcome::SingleCandidate { candidate } => vec![candidate.name.as_str()],
            CandidateOutcome::MultipleCandidates { candidates } => {
                candidates.iter().map(|c| c.name.as_str()).collect()
            }
            other => panic!("{written}: expected candidates, got {other:?}"),
        };
        assert_eq!(names, expected, "{written}");
    }
}

fn assert_no_candidate(records: &[CallCandidateRecord], written: &str, reason_part: &str) {
    let calls = calls_to(records, written);
    assert!(!calls.is_empty(), "no record for {written}");
    for record in &calls {
        match &record.outcome {
            CandidateOutcome::NoCandidate { reason } => {
                assert!(reason.contains(reason_part), "{written}: {reason}")
            }
            other => panic!("{written}: expected no_candidate, got {other:?}"),
        }
    }
}

fn assert_out_of_scope(records: &[CallCandidateRecord], written: &str) {
    let calls = calls_to(records, written);
    assert!(!calls.is_empty(), "no record for {written}");
    for record in &calls {
        assert!(
            matches!(record.outcome, CandidateOutcome::OutOfScope { .. }),
            "{written}: expected out_of_scope, got {:?}",
            record.outcome
        );
    }
}

// ---------------------------------------------------------------------------
// crate:: roots
// ---------------------------------------------------------------------------

#[test]
fn crate_root_function() {
    let records = records(&[(
        "src/lib.rs",
        "fn helper() {}\nfn run() { crate::helper(); }\n",
    )]);
    assert_candidates(&records, "crate::helper", &["helper"]);
}

#[test]
fn crate_module_function() {
    let records = records(&[
        (
            "src/lib.rs",
            "mod util;\nfn run() { crate::util::helper(); }\n",
        ),
        ("src/util.rs", "pub fn helper() {}\n"),
    ]);
    assert_candidates(&records, "crate::util::helper", &["helper"]);
    assert!(evidence(calls_to(&records, "crate::util::helper")[0])
        .contains("written=crate::util::helper"));
}

#[test]
fn crate_nested_module_function() {
    let records = records(&[
        (
            "src/lib.rs",
            "mod a;\nfn run() { crate::a::b::helper(); }\n",
        ),
        ("src/a.rs", "pub mod b;\n"),
        ("src/a/b.rs", "pub fn helper() {}\n"),
    ]);
    assert_candidates(&records, "crate::a::b::helper", &["helper"]);
}

// ---------------------------------------------------------------------------
// self:: / super:: roots
// ---------------------------------------------------------------------------

#[test]
fn self_root_function() {
    let records = records(&[(
        "src/lib.rs",
        "fn helper() {}\nfn run() { self::helper(); }\n",
    )]);
    assert_candidates(&records, "self::helper", &["helper"]);
}

#[test]
fn self_nested_module_function() {
    let records = records(&[(
        "src/lib.rs",
        "mod inner { pub fn helper() {} }\nmod outer { fn run() { self::inner::helper(); } }\n",
    )]);
    // `self` inside `outer` names `outer`; `outer::inner` is not a module.
    assert_no_candidate(&records, "self::inner::helper", "");
}

#[test]
fn super_root_function() {
    let records = records(&[(
        "src/lib.rs",
        "fn helper() {}\nmod m { pub fn run() { super::helper(); } }\n",
    )]);
    assert_candidates(&records, "super::helper", &["helper"]);
}

#[test]
fn super_super_root_function() {
    let records = records(&[(
        "src/lib.rs",
        "fn helper() {}\nmod a { pub mod b { pub fn run() { super::super::helper(); } } }\n",
    )]);
    assert_candidates(&records, "super::super::helper", &["helper"]);
}

#[test]
fn super_beyond_crate_root_is_bounded() {
    let records = records(&[("src/lib.rs", "fn run() { super::helper(); }\n")]);
    // `super` from the crate root walks above it — bounded, never guessed.
    assert_out_of_scope(&records, "super::helper");
}

// ---------------------------------------------------------------------------
// relative module roots
// ---------------------------------------------------------------------------

#[test]
fn relative_local_module_function() {
    let records = records(&[(
        "src/lib.rs",
        "mod util { pub fn helper() {} }\nfn run() { util::helper(); }\n",
    )]);
    assert_candidates(&records, "util::helper", &["helper"]);
}

#[test]
fn relative_nested_module_function() {
    let records = records(&[(
        "src/lib.rs",
        "mod a { pub mod b { pub fn helper() {} } }\nfn run() { a::b::helper(); }\n",
    )]);
    assert_candidates(&records, "a::b::helper", &["helper"]);
}

#[test]
fn external_module_file_function() {
    let records = records(&[
        ("src/lib.rs", "mod util;\nfn run() { util::helper(); }\n"),
        ("src/util.rs", "pub fn helper() {}\n"),
    ]);
    assert_candidates(&records, "util::helper", &["helper"]);
}

// ---------------------------------------------------------------------------
// terminal / root non-candidates
// ---------------------------------------------------------------------------

#[test]
fn terminal_non_function_is_no_candidate() {
    let records = records(&[
        (
            "src/lib.rs",
            "mod util;\nfn run() { crate::util::Thing(); }\n",
        ),
        ("src/util.rs", "pub struct Thing;\n"),
    ]);
    assert_no_candidate(&records, "crate::util::Thing", "not a `function`");
}

#[test]
fn type_path_call_is_not_a_module_candidate() {
    // `Vec::new()` — `Vec` is not a structural module child, so no candidate.
    let records = records(&[("src/lib.rs", "fn run() { Vec::new(); }\n")]);
    assert_out_of_scope(&records, "Vec::new");
}

#[test]
fn external_crate_path_is_out_of_scope() {
    let records = records(&[
        ("src/lib.rs", "mod util;\nfn run() { std::mem::drop(x); }\n"),
        ("src/util.rs", "pub fn helper() {}\n"),
    ]);
    assert_out_of_scope(&records, "std::mem::drop");
}

#[test]
fn imported_module_alias_is_not_followed() {
    // `use crate::util as u` binds `u` as an import — `u::helper()` must not be
    // treated as a structural module path.
    let records = records(&[
        (
            "src/lib.rs",
            "mod util;\nuse crate::util as u;\nfn run() { u::helper(); }\n",
        ),
        ("src/util.rs", "pub fn helper() {}\n"),
    ]);
    assert_no_candidate(&records, "u::helper", "shadowed");
}

// ---------------------------------------------------------------------------
// root shadowing
// ---------------------------------------------------------------------------

#[test]
fn local_value_shadows_module_root() {
    let records = records(&[(
        "src/lib.rs",
        "mod util { pub fn helper() {} }\nfn run() { let util = 0; util::helper(); }\n",
    )]);
    assert_no_candidate(&records, "util::helper", "shadowed");
}

#[test]
fn parameter_shadows_module_root() {
    let records = records(&[(
        "src/lib.rs",
        "mod util { pub fn helper() {} }\nfn run(util: u32) { util::helper(); }\n",
    )]);
    assert_no_candidate(&records, "util::helper", "shadowed");
}

#[test]
fn import_shadows_module_root() {
    let records = records(&[
        (
            "src/lib.rs",
            "mod util { pub fn helper() {} }\nfn run() { use crate::other::util; util::helper(); }\n",
        ),
        ("src/other.rs", "pub mod util;\n"),
        ("src/other/util.rs", "pub fn helper() {}\n"),
    ]);
    assert_no_candidate(&records, "util::helper", "shadowed");
}

// ---------------------------------------------------------------------------
// ambiguity / cfg / path-locality
// ---------------------------------------------------------------------------

#[test]
fn ambiguous_cfg_modules_keep_both_candidates() {
    // Two `mod util` bodies gated by cfg both persist structurally.
    let records = records(&[(
        "src/lib.rs",
        "#[cfg(unix)]\nmod util { pub fn helper() {} }\n#[cfg(windows)]\nmod util { pub fn helper() {} }\nfn run() { util::helper(); }\n",
    )]);
    let calls = calls_to(&records, "util::helper");
    assert!(matches!(
        calls[0].outcome,
        CandidateOutcome::MultipleCandidates { .. }
    ));
}

#[test]
fn qualified_path_does_not_borrow_unrelated_same_name() {
    // `util::helper` resolves to util::helper, not the crate-level `helper`.
    let records = records(&[(
        "src/lib.rs",
        "fn helper() {}\nmod util { pub fn helper() {} }\nfn run() { util::helper(); }\n",
    )]);
    let calls = calls_to(&records, "util::helper");
    match &calls[0].outcome {
        CandidateOutcome::SingleCandidate { candidate } => {
            assert!(
                candidate.scope_path.contains("util"),
                "{:?}",
                candidate.scope_path
            )
        }
        other => panic!("expected single, got {other:?}"),
    }
}
