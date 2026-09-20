//! TASK 3C V2: lexical-shadowing correction regression suite.
//!
//! V1 emitted an outer `function` candidate for a plain-name call even when a
//! closer local `let`/parameter/pattern binding owned the written name in that
//! source region (finding `T3C-F005`). V2 consumes the persisted
//! `LocalBindingOccurrence` facts (plus `const`/`static` declarations and `use`
//! imports) so a covering same-name blocker suppresses the unsafe outer
//! candidate.
//!
//! These tests drive `rule_rust::candidates` directly on an in-memory
//! `FileAnalysis`, so each case is a self-contained source string — no
//! snapshot/link pipeline is required to prove the shadowing behaviour.
//!
//! The outcomes asserted are **dispositions**, not resolutions: a
//! `no_candidate` here means "the bounded rule will not name an unsafe outer
//! function", never "the call has no runtime target".

mod support;

use repodex::candidates::{rule_rust, CallCandidateRecord, CandidateOutcome};
use repodex::model::LanguageId;
use support::analyze_source;

/// Derive the candidate records for one Rust source string.
fn candidates_for(source: &str) -> Vec<CallCandidateRecord> {
    let analysis = analyze_source(LanguageId::Rust, "t.rs", source.as_bytes());
    // The lexical-shadowing tests exercise the local blockers only; no TASK 3B
    // link relationships are needed, so the import lookup is empty.
    rule_rust::candidates(&[analysis], &[])
}

/// The records for every call to `written`, in source order.
fn calls_to<'a>(records: &'a [CallCandidateRecord], written: &str) -> Vec<&'a CallCandidateRecord> {
    records
        .iter()
        .filter(|record| record.written == written)
        .collect()
}

/// Whether the record suppressed the candidate for the given blocker reason.
fn is_blocked(record: &CallCandidateRecord, reason: &str) -> bool {
    matches!(&record.outcome, CandidateOutcome::NoCandidate { reason: r } if r == reason)
}

/// The single candidate's declaration name, when the outcome is a single.
fn single_name(record: &CallCandidateRecord) -> Option<String> {
    record
        .outcome
        .single()
        .map(|candidate| candidate.name.clone())
}

/// Assert the `index`-th call to `written` is `NoCandidate` with `reason`.
fn assert_blocked(records: &[CallCandidateRecord], written: &str, index: usize, reason: &str) {
    let record = calls_to(records, written)[index];
    assert!(
        is_blocked(record, reason),
        "expected `{written}` call #{index} blocked by `{reason}`, got {:?}",
        record.outcome
    );
    // Provenance must name the blocker, never be opaque.
    assert!(
        record
            .provenance
            .evidence
            .iter()
            .any(|line| line.starts_with("blocker=")),
        "blocked record for `{written}` lacks blocker provenance: {:?}",
        record.provenance.evidence
    );
}

/// Assert the `index`-th call to `written` is `SingleCandidate` for `name`.
fn assert_single(records: &[CallCandidateRecord], written: &str, index: usize, name: &str) {
    let record = calls_to(records, written)[index];
    assert_eq!(
        single_name(record).as_deref(),
        Some(name),
        "expected `{written}` call #{index} -> single candidate `{name}`, got {:?}",
        record.outcome
    );
}

// ---------------------------------------------------------------------------
// `let` shadowing (mandatory §9 cases)
// ---------------------------------------------------------------------------

#[test]
fn let_binding_blocks_outer_function() {
    let records =
        candidates_for("fn helper() {}\nfn run() {\n    let helper = || {};\n    helper();\n}\n");
    assert_blocked(&records, "helper", 0, "shadowed_by_local_binding");
}

#[test]
fn call_before_let_is_not_blocked() {
    let records =
        candidates_for("fn helper() {}\nfn run() {\n    helper();\n    let helper = || {};\n}\n");
    assert_single(&records, "helper", 0, "helper");
}

#[test]
fn let_does_not_cover_its_own_initializer() {
    let records = candidates_for("fn helper() {}\nfn run() {\n    let helper = helper();\n}\n");
    assert_single(&records, "helper", 0, "helper");
}

#[test]
fn nested_block_bounds_let_visibility() {
    let records = candidates_for(
        "fn helper() {}\nfn run() {\n    {\n        let helper = || {};\n        helper();\n    }\n    helper();\n}\n",
    );
    assert_blocked(&records, "helper", 0, "shadowed_by_local_binding");
    assert_single(&records, "helper", 1, "helper");
}

#[test]
fn sequential_lets_each_block() {
    let records = candidates_for(
        "fn helper() {}\nfn run() {\n    let helper = f1;\n    helper();\n    let helper = f2;\n    helper();\n}\n",
    );
    assert_blocked(&records, "helper", 0, "shadowed_by_local_binding");
    assert_blocked(&records, "helper", 1, "shadowed_by_local_binding");
}

#[test]
fn let_does_not_reach_into_a_nested_function() {
    // `let helper` is a local in `run`; `fn inner` opens a fresh scope, so the
    // `helper()` inside it resolves to the module function, not the binding.
    let records = candidates_for(
        "fn helper() {}\nfn run() {\n    let helper = || {};\n    fn inner() {\n        helper();\n    }\n}\n",
    );
    assert_single(&records, "helper", 0, "helper");
}

#[test]
fn let_is_captured_by_a_later_closure() {
    // A closure defined after the `let` captures it, so `helper()` inside is
    // blocked even though the call is inside a nested closure scope.
    let records = candidates_for(
        "fn helper() {}\nfn run() {\n    let helper = || {};\n    let f = || helper();\n}\n",
    );
    assert_blocked(&records, "helper", 0, "shadowed_by_local_binding");
}

#[test]
fn closure_defined_before_let_is_not_captured() {
    // The closure binds `helper` at its definition point, before `let helper`
    // exists, so it still sees the module function.
    let records = candidates_for(
        "fn helper() {}\nfn run() {\n    let f = || helper();\n    let helper = || {};\n}\n",
    );
    assert_single(&records, "helper", 0, "helper");
}

// ---------------------------------------------------------------------------
// Parameters and pattern bindings
// ---------------------------------------------------------------------------

#[test]
fn function_parameter_blocks_outer_function() {
    let records = candidates_for(
        "fn helper() {}\nfn run(helper: fn()) {\n    helper();\n}\nfn other() { helper(); }\n",
    );
    assert_blocked(&records, "helper", 0, "shadowed_by_local_binding");
    // The parameter must not leak outside `run`.
    assert_single(&records, "helper", 1, "helper");
}

#[test]
fn signature_only_parameter_binds_nothing() {
    // A bodiless signature parameter has empty visibility, so it blocks nothing.
    let records = candidates_for(
        "trait T { fn run(helper: fn()); }\nfn helper() {}\nfn free() { helper(); }\n",
    );
    assert_single(&records, "helper", 0, "helper");
}

#[test]
fn closure_parameter_blocks_inside_body_only() {
    let records = candidates_for(
        "fn helper() {}\nfn outer() {\n    let f = |helper| {\n        helper();\n    };\n    helper();\n}\n",
    );
    assert_blocked(&records, "helper", 0, "shadowed_by_local_binding");
    assert_single(&records, "helper", 1, "helper");
}

#[test]
fn for_pattern_blocks_loop_body_only() {
    let records = candidates_for(
        "fn helper() {}\nfn run(helpers: Vec<fn()>) {\n    for helper in helpers {\n        helper();\n    }\n    helper();\n}\n",
    );
    assert_blocked(&records, "helper", 0, "shadowed_by_local_binding");
    assert_single(&records, "helper", 1, "helper");
}

#[test]
fn match_arm_binding_blocks_that_arm_only() {
    let records = candidates_for(
        "fn helper() {}\nfn run(v: Option<fn()>) {\n    match v {\n        Some(helper) => helper(),\n        None => helper(),\n    }\n}\n",
    );
    // `Some(helper)` binds in a refutable position: ambiguous, so it blocks
    // conservatively; the `None` arm binds nothing.
    assert_blocked(&records, "helper", 0, "blocked_by_ambiguous_local_binding");
    assert_single(&records, "helper", 1, "helper");
}

#[test]
fn match_at_capture_is_a_definite_binding() {
    // `helper @ pattern` is a definite capture even in a refutable arm.
    let records = candidates_for(
        "fn helper() {}\nfn run(v: Option<fn()>) {\n    match v {\n        helper @ Some(_) => helper(),\n        _ => helper(),\n    }\n}\n",
    );
    assert_blocked(&records, "helper", 0, "shadowed_by_local_binding");
    assert_single(&records, "helper", 1, "helper");
}

#[test]
fn if_let_blocks_consequence_not_else_or_after() {
    let records = candidates_for(
        "fn helper() {}\nfn run(v: Option<fn()>) {\n    if let Some(helper) = v {\n        helper();\n    } else {\n        helper();\n    }\n    helper();\n}\n",
    );
    assert_blocked(&records, "helper", 0, "blocked_by_ambiguous_local_binding");
    assert_single(&records, "helper", 1, "helper");
    assert_single(&records, "helper", 2, "helper");
}

#[test]
fn while_let_blocks_loop_body_only() {
    let records = candidates_for(
        "fn helper() {}\nfn run(v: Option<fn()>) {\n    while let Some(helper) = v {\n        helper();\n    }\n    helper();\n}\n",
    );
    assert_blocked(&records, "helper", 0, "blocked_by_ambiguous_local_binding");
    assert_single(&records, "helper", 1, "helper");
}

#[test]
fn ambiguous_refutable_binding_blocks_conservatively() {
    // A bare `helper` match arm is binding-like but could name a const/variant;
    // suppress the outer function rather than manufacture certainty.
    let records = candidates_for(
        "fn helper() {}\nfn run(v: Option<fn()>) {\n    match v {\n        helper => helper(),\n        _ => helper(),\n    }\n}\n",
    );
    assert_blocked(&records, "helper", 0, "blocked_by_ambiguous_local_binding");
    assert_single(&records, "helper", 1, "helper");
}

// ---------------------------------------------------------------------------
// Non-blockers and item blockers
// ---------------------------------------------------------------------------

#[test]
fn unrelated_local_name_does_not_block() {
    let records =
        candidates_for("fn helper() {}\nfn run() {\n    let other = || {};\n    helper();\n}\n");
    assert_single(&records, "helper", 0, "helper");
}

#[test]
fn nested_function_remains_a_candidate() {
    let records =
        candidates_for("fn helper() {}\nfn run() {\n    fn helper() {}\n    helper();\n}\n");
    assert_single(&records, "helper", 0, "helper");
}

#[test]
fn use_import_blocks_outer_function() {
    let records = candidates_for(
        "fn helper() {}\nfn run() {\n    use crate::other::helper;\n    helper();\n}\n",
    );
    assert_blocked(&records, "helper", 0, "blocked_by_import_binding");
}

#[test]
fn use_alias_blocks_under_the_alias_name() {
    let records = candidates_for(
        "fn helper() {}\nfn run() {\n    use crate::other::real as helper;\n    helper();\n}\n",
    );
    assert_blocked(&records, "helper", 0, "blocked_by_import_binding");
}

#[test]
fn use_import_is_scoped_to_its_function() {
    let records = candidates_for(
        "fn helper() {}\nfn a() { use x::helper; helper(); }\nfn b() { helper(); }\n",
    );
    assert_blocked(&records, "helper", 0, "blocked_by_import_binding");
    assert_single(&records, "helper", 1, "helper");
}

#[test]
fn use_in_a_nested_module_blocks_the_outer_function() {
    // A `use` inside `mod m` is nearer than the file-level `fn helper`, so it
    // shadows it for calls inside `m`.
    let records = candidates_for(
        "fn helper() {}\nmod m {\n    use crate::other::helper;\n    fn run() { helper(); }\n}\n",
    );
    assert_blocked(&records, "helper", 0, "blocked_by_import_binding");
}

#[test]
fn same_level_use_does_not_shadow_a_function() {
    // `use` and `fn` at the same level cannot both bind a value (E0255), so a
    // same-level `use` must be a non-value namespace import (e.g. a macro) and
    // the `function` wins for a `()` call.
    let records =
        candidates_for("use crate::other::helper;\nfn helper() {}\nfn run() { helper(); }\n");
    assert_single(&records, "helper", 0, "helper");
}

#[test]
fn wildcard_use_does_not_block_a_specific_name() {
    // `use a::*` binds no single written name, so it cannot prove `helper` is
    // imported; it does not block in V2.
    let records =
        candidates_for("fn helper() {}\nfn run() {\n    use crate::other::*;\n    helper();\n}\n");
    assert_single(&records, "helper", 0, "helper");
}

#[test]
fn local_const_blocks_outer_function() {
    let records = candidates_for(
        "fn helper() {}\nfn run() {\n    const helper: fn() = a;\n    helper();\n}\n",
    );
    assert_blocked(&records, "helper", 0, "blocked_by_local_constant");
}

#[test]
fn blocker_provenance_names_the_blocker() {
    let records =
        candidates_for("fn helper() {}\nfn run() {\n    let helper = || {};\n    helper();\n}\n");
    let record = calls_to(&records, "helper")[0];
    let evidence = record.provenance.evidence.join("\n");
    assert!(evidence.contains("blocker=local_binding"));
    assert!(evidence.contains("blocker_kind=let"));
    assert!(evidence.contains("blocker_name=helper"));
    assert!(evidence.contains("blocker_ambiguous=false"));
}
