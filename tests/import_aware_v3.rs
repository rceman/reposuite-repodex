//! TASK 3D — `rust.call.imported_function_candidate`.
//!
//! A plain-name Rust call blocked by an explicit `use` binding is resolved
//! through that import occurrence's persisted TASK 3B `use_path` relationship
//! into zero/one/many `function` candidates. These tests drive
//! `rule_rust::candidates` directly on in-memory `FileAnalysis` values plus
//! hand-built `LinkRecord`s, so each link outcome can be exercised
//! deterministically without a full link build.

mod support;

use repodex::candidates::{candidate_rule, rule_rust, CallCandidateRecord, CandidateOutcome};
use repodex::links::{FactKind, FactLocator, LinkOutcome, LinkProvenance, LinkRecord, LinkTarget};
use repodex::model::{FileAnalysis, LanguageId};
use support::analyze_source;

const CALLER: &str = "src/lib.rs";
const TARGET: &str = "src/util.rs";

/// Analyze the caller file plus each `(path, source)` target file.
fn analyses(caller_src: &str, targets: &[(&str, &str)]) -> Vec<FileAnalysis> {
    let mut all = vec![analyze_source(
        LanguageId::Rust,
        CALLER,
        caller_src.as_bytes(),
    )];
    for (path, source) in targets {
        all.push(analyze_source(LanguageId::Rust, path, source.as_bytes()));
    }
    all
}

/// Build the `use_path` link for one caller import item.
fn link(import_id: u32, item_index: u32, written: &str, outcome: LinkOutcome) -> LinkRecord {
    let source = FactLocator {
        relative_path: CALLER.to_string(),
        fact_kind: FactKind::Import,
        fact_id: import_id,
        item_index: Some(item_index),
    };
    LinkRecord::new(
        "use_path",
        source.clone(),
        written,
        outcome,
        LinkProvenance {
            language: "rust".to_string(),
            rule_id: "rust.use.crate_path".to_string(),
            source,
            written: written.to_string(),
            evidence: Vec::new(),
            metadata: Vec::new(),
        },
    )
}

/// A `Declaration` link target for a `function` in `path` at `declaration_id`.
fn fn_target(path: &str, declaration_id: u32, name: &str) -> LinkTarget {
    LinkTarget::declaration(path, declaration_id, "crate::x", "function", name)
}

/// Records for every call to `written`, in source order.
fn calls_to<'a>(records: &'a [CallCandidateRecord], written: &str) -> Vec<&'a CallCandidateRecord> {
    records
        .iter()
        .filter(|record| record.written == written)
        .collect()
}

fn evidence(record: &CallCandidateRecord) -> String {
    record.provenance.evidence.join("\n")
}

/// Assert the `index`-th call to `written` is `NoCandidate` with `reason`.
fn assert_no_candidate(
    records: &[CallCandidateRecord],
    written: &str,
    index: usize,
    reason: &str,
) -> CallCandidateRecord {
    let calls = calls_to(records, written);
    let record = calls[index].clone();
    match &record.outcome {
        CandidateOutcome::NoCandidate { reason: got } => {
            assert_eq!(got, reason, "call `{written}` reason")
        }
        other => panic!("expected NoCandidate({reason}) for `{written}`, got {other:?}"),
    }
    record
}

/// Assert the `index`-th call to `written` is `SingleCandidate` named `name`.
fn assert_single(records: &[CallCandidateRecord], written: &str, index: usize, name: &str) {
    let calls = calls_to(records, written);
    match &calls[index].outcome {
        CandidateOutcome::SingleCandidate { candidate } => {
            assert_eq!(candidate.name, name, "call `{written}` candidate")
        }
        other => panic!("expected SingleCandidate({name}) for `{written}`, got {other:?}"),
    }
}

/// Assert the `index`-th call to `written` is `MultipleCandidates` with `names`.
fn assert_multiple(records: &[CallCandidateRecord], written: &str, index: usize, names: &[&str]) {
    let calls = calls_to(records, written);
    match &calls[index].outcome {
        CandidateOutcome::MultipleCandidates { candidates } => {
            let got: Vec<&str> = candidates.iter().map(|c| c.name.as_str()).collect();
            assert_eq!(got, names, "call `{written}` candidates");
        }
        other => panic!("expected MultipleCandidates({names:?}) for `{written}`, got {other:?}"),
    }
}

/// Assert the `index`-th call to `written` uses the imported-candidate rule id.
fn assert_import_rule(records: &[CallCandidateRecord], written: &str, index: usize) {
    let calls = calls_to(records, written);
    assert_eq!(
        calls[index].rule_id,
        candidate_rule::RUST_CALL_IMPORTED_FUNCTION_CANDIDATE,
        "call `{written}` rule id"
    );
}

// ---------------------------------------------------------------------------
// TASK 3B outcome -> candidate
// ---------------------------------------------------------------------------

#[test]
fn exact_link_to_function_is_a_single_candidate() {
    let records = rule_rust::candidates(
        &analyses(
            "use crate::util::helper;\nfn run() { helper(); }\n",
            &[(TARGET, "pub fn helper() {}\n")],
        ),
        &[link(
            0,
            0,
            "crate::util::helper",
            LinkOutcome::exact(fn_target(TARGET, 0, "helper")),
        )],
    );
    assert_single(&records, "helper", 0, "helper");
    assert_import_rule(&records, "helper", 0);
    let ev = evidence(calls_to(&records, "helper")[0]);
    assert!(ev.contains("link_outcome=exact"), "{ev}");
    assert!(ev.contains("import_link_id="), "{ev}");
}

#[test]
fn exact_link_to_non_function_is_no_candidate() {
    let records = rule_rust::candidates(
        &analyses(
            "use crate::util::helper;\nfn run() { helper(); }\n",
            // decl 0 is a `struct`, not a `function`.
            &[(TARGET, "pub struct helper;\n")],
        ),
        &[link(
            0,
            0,
            "crate::util::helper",
            LinkOutcome::exact(LinkTarget::declaration(
                TARGET,
                0,
                "crate::util",
                "struct",
                "helper",
            )),
        )],
    );
    let record = assert_no_candidate(&records, "helper", 0, "import_target_not_eligible_function");
    assert_import_rule(&records, "helper", 0);
    assert!(evidence(&record).contains("excluded_non_function"));
}

#[test]
fn ambiguous_link_filters_to_functions() {
    let records = rule_rust::candidates(
        &analyses(
            "use crate::util::helper;\nfn run() { helper(); }\n",
            &[
                (TARGET, "pub fn helper() {}\npub struct helper2;\n"),
                ("src/other.rs", "pub fn helper() {}\n"),
            ],
        ),
        &[link(
            0,
            0,
            "crate::util::helper",
            LinkOutcome::Ambiguous {
                candidates: vec![
                    fn_target(TARGET, 0, "helper"),
                    // a non-function structural candidate that must be filtered out
                    LinkTarget::declaration(TARGET, 1, "crate::util", "struct", "helper2"),
                    fn_target("src/other.rs", 0, "helper"),
                ],
            },
        )],
    );
    // Two eligible functions survive the filter -> multiple_candidates, with
    // upstream `ambiguous` preserved in provenance.
    assert_multiple(&records, "helper", 0, &["helper", "helper"]);
    let ev = evidence(calls_to(&records, "helper")[0]);
    assert!(ev.contains("link_outcome=ambiguous"), "{ev}");
    assert!(ev.contains("excluded_non_function_count=1"), "{ev}");
}

#[test]
fn ambiguous_link_with_one_function_is_single_but_provenance_keeps_ambiguous() {
    let records = rule_rust::candidates(
        &analyses(
            "use crate::util::helper;\nfn run() { helper(); }\n",
            &[(TARGET, "pub fn helper() {}\npub struct helper2;\n")],
        ),
        &[link(
            0,
            0,
            "crate::util::helper",
            LinkOutcome::Ambiguous {
                candidates: vec![
                    fn_target(TARGET, 0, "helper"),
                    LinkTarget::declaration(TARGET, 1, "crate::util", "struct", "helper2"),
                ],
            },
        )],
    );
    // One eligible function survives -> single_candidate, but the record must
    // still say the upstream relationship was Ambiguous.
    assert_single(&records, "helper", 0, "helper");
    let ev = evidence(calls_to(&records, "helper")[0]);
    assert!(ev.contains("link_outcome=ambiguous"), "{ev}");
}

#[test]
fn ambiguous_link_with_no_functions_is_no_candidate() {
    let records = rule_rust::candidates(
        &analyses(
            "use crate::util::helper;\nfn run() { helper(); }\n",
            &[(TARGET, "pub struct helper;\npub const helper: u32 = 0;\n")],
        ),
        &[link(
            0,
            0,
            "crate::util::helper",
            LinkOutcome::Ambiguous {
                candidates: vec![
                    LinkTarget::declaration(TARGET, 0, "crate::util", "struct", "helper"),
                    LinkTarget::declaration(TARGET, 1, "crate::util", "const", "helper"),
                ],
            },
        )],
    );
    assert_no_candidate(&records, "helper", 0, "import_target_not_eligible_function");
}

#[test]
fn unresolved_link_is_no_candidate() {
    let records = rule_rust::candidates(
        &analyses("use crate::util::helper;\nfn run() { helper(); }\n", &[]),
        &[link(
            0,
            0,
            "crate::util::helper",
            LinkOutcome::unresolved("no such module"),
        )],
    );
    assert_no_candidate(&records, "helper", 0, "import_structurally_unresolved");
    assert_import_rule(&records, "helper", 0);
}

#[test]
fn out_of_scope_link_is_no_candidate() {
    let records = rule_rust::candidates(
        &analyses("use std::other::helper;\nfn run() { helper(); }\n", &[]),
        &[link(
            0,
            0,
            "std::other::helper",
            LinkOutcome::out_of_scope("not a crate path"),
        )],
    );
    assert_no_candidate(&records, "helper", 0, "import_out_of_scope");
    assert_import_rule(&records, "helper", 0);
}

#[test]
fn missing_link_keeps_the_conservative_import_blocker() {
    let records = rule_rust::candidates(
        &analyses("use crate::util::helper;\nfn run() { helper(); }\n", &[]),
        &[],
    );
    assert_no_candidate(&records, "helper", 0, "blocked_by_import_binding");
}

#[test]
fn glob_import_never_produces_a_candidate() {
    // A `use a::*` binds no single name, so it cannot block — the call stays a
    // normal no-candidate under the local rule, never an import-aware record.
    let records = rule_rust::candidates(
        &analyses("use crate::util::*;\nfn run() { helper(); }\n", &[]),
        &[link(
            0,
            0,
            "crate::util::*",
            LinkOutcome::exact(LinkTarget::file(TARGET)),
        )],
    );
    let calls = calls_to(&records, "helper");
    match &calls[0].outcome {
        CandidateOutcome::NoCandidate { reason } => {
            assert!(reason.starts_with("no `function` declaration"), "{reason}")
        }
        other => panic!("expected ordinary no_candidate, got {other:?}"),
    }
    assert_eq!(
        calls[0].rule_id,
        candidate_rule::RUST_CALL_LOCAL_FUNCTION_CANDIDATE
    );
}

// ---------------------------------------------------------------------------
// Lexical precedence beats the import (§15, §16, §17, §24, §31, §32)
// ---------------------------------------------------------------------------

#[test]
fn a_closer_let_binding_beats_the_import() {
    let records = rule_rust::candidates(
        &analyses(
            "use crate::util::helper;\nfn run() {\n    let helper = || {};\n    helper();\n}\n",
            &[(TARGET, "pub fn helper() {}\n")],
        ),
        &[link(
            0,
            0,
            "crate::util::helper",
            LinkOutcome::exact(fn_target(TARGET, 0, "helper")),
        )],
    );
    assert_no_candidate(&records, "helper", 0, "shadowed_by_local_binding");
}

#[test]
fn a_closer_function_parameter_beats_the_import() {
    let records = rule_rust::candidates(
        &analyses(
            "use crate::util::helper;\nfn run(helper: fn()) { helper(); }\n",
            &[(TARGET, "pub fn helper() {}\n")],
        ),
        &[link(
            0,
            0,
            "crate::util::helper",
            LinkOutcome::exact(fn_target(TARGET, 0, "helper")),
        )],
    );
    assert_no_candidate(&records, "helper", 0, "shadowed_by_local_binding");
}

#[test]
fn a_nearer_nested_function_beats_the_import() {
    let records = rule_rust::candidates(
        &analyses(
            "use crate::util::helper;\nfn run() {\n    fn helper() {}\n    helper();\n}\n",
            &[(TARGET, "pub fn helper() {}\n")],
        ),
        &[link(
            0,
            0,
            "crate::util::helper",
            LinkOutcome::exact(fn_target(TARGET, 0, "helper")),
        )],
    );
    assert_single(&records, "helper", 0, "helper");
    // The nearer lexical `fn` wins under the local rule, not the import.
    assert_eq!(
        calls_to(&records, "helper")[0].rule_id,
        candidate_rule::RUST_CALL_LOCAL_FUNCTION_CANDIDATE
    );
}

#[test]
fn a_call_before_a_later_let_can_use_the_import() {
    let records = rule_rust::candidates(
        &analyses(
            "use crate::util::helper;\nfn run() {\n    helper();\n    let helper = || {};\n}\n",
            &[(TARGET, "pub fn helper() {}\n")],
        ),
        &[link(
            0,
            0,
            "crate::util::helper",
            LinkOutcome::exact(fn_target(TARGET, 0, "helper")),
        )],
    );
    // The first call precedes the `let`, so the import still owns the name.
    assert_single(&records, "helper", 0, "helper");
    assert_import_rule(&records, "helper", 0);
}

#[test]
fn an_import_outside_lexical_applicability_does_not_link() {
    // `use` inside `a` binds `helper` only in `a`; the call in `b` sees no
    // import and falls to the ordinary no-function outcome.
    let records = rule_rust::candidates(
        &analyses(
            "mod a {\n    use crate::util::helper;\n}\nmod b {\n    pub fn run() { helper(); }\n}\n",
            &[(TARGET, "pub fn helper() {}\n")],
        ),
        &[link(0, 0, "crate::util::helper", LinkOutcome::exact(fn_target(TARGET, 0, "helper")))],
    );
    let calls = calls_to(&records, "helper");
    assert_eq!(calls.len(), 1);
    match &calls[0].outcome {
        CandidateOutcome::NoCandidate { reason } => {
            assert!(reason.starts_with("no `function` declaration"), "{reason}")
        }
        other => panic!("expected ordinary no_candidate, got {other:?}"),
    }
}

#[test]
fn an_unresolved_import_does_not_borrow_an_outer_function() {
    // §24: once the nearer `use` owns the name, an unrelated outer same-name
    // `fn` must NOT be borrowed even when the import resolves to nothing.
    let records = rule_rust::candidates(
        &analyses(
            "fn helper() {}\nfn run() {\n    use crate::types::helper;\n    helper();\n}\n",
            &[],
        ),
        &[link(
            0,
            0,
            "crate::types::helper",
            LinkOutcome::unresolved("no such decl"),
        )],
    );
    assert_no_candidate(&records, "helper", 0, "import_structurally_unresolved");
}

#[test]
fn a_local_function_blocker_also_beats_the_import() {
    let records = rule_rust::candidates(
        &analyses(
            "use crate::util::helper;\nfn run() {\n    const helper: u32 = 0;\n    helper();\n}\n",
            &[(TARGET, "pub fn helper() {}\n")],
        ),
        &[link(
            0,
            0,
            "crate::util::helper",
            LinkOutcome::exact(fn_target(TARGET, 0, "helper")),
        )],
    );
    assert_no_candidate(&records, "helper", 0, "blocked_by_local_constant");
}

// ---------------------------------------------------------------------------
// Alias / grouped forms (§7)
// ---------------------------------------------------------------------------

#[test]
fn an_aliased_import_binds_the_alias_not_the_target() {
    let records = rule_rust::candidates(
        &analyses(
            "use crate::util::helper as execute;\nfn run() { execute(); }\n",
            &[(TARGET, "pub fn helper() {}\n")],
        ),
        &[link(
            0,
            0,
            "crate::util::helper",
            LinkOutcome::exact(fn_target(TARGET, 0, "helper")),
        )],
    );
    // The local call name `execute` binds the alias; the candidate name is the
    // imported target `helper`.
    assert_single(&records, "execute", 0, "helper");
    assert_import_rule(&records, "execute", 0);
}

#[test]
fn a_grouped_import_resolves_the_matching_item() {
    let records = rule_rust::candidates(
        &analyses(
            "use crate::util::{helper, other};\nfn run() { helper(); }\n",
            &[(TARGET, "pub fn helper() {}\npub fn other() {}\n")],
        ),
        &[
            link(
                0,
                0,
                "crate::util::helper",
                LinkOutcome::exact(fn_target(TARGET, 0, "helper")),
            ),
            link(
                0,
                1,
                "crate::util::other",
                LinkOutcome::exact(fn_target(TARGET, 1, "other")),
            ),
        ],
    );
    assert_single(&records, "helper", 0, "helper");
    assert_import_rule(&records, "helper", 0);
}

#[test]
fn a_grouped_aliased_import_uses_the_alias() {
    let records = rule_rust::candidates(
        &analyses(
            "use crate::util::{helper as run_helper};\nfn run() { run_helper(); }\n",
            &[(TARGET, "pub fn helper() {}\n")],
        ),
        &[link(
            0,
            0,
            "crate::util::helper",
            LinkOutcome::exact(fn_target(TARGET, 0, "helper")),
        )],
    );
    assert_single(&records, "run_helper", 0, "helper");
}

// ---------------------------------------------------------------------------
// Re-export limitation (§14) and same-level policy (§19)
// ---------------------------------------------------------------------------

#[test]
fn a_reexport_chain_is_not_followed() {
    // `pub use crate::internal::helper` in `util` re-exports `helper`. A
    // consumer `use crate::util::helper` resolves through the DIRECT link to
    // the re-export declaration — but the re-export decl is a `use`, not a
    // `function`, so it is not an eligible candidate (the transitive target is
    // never followed).
    let records = rule_rust::candidates(
        &analyses(
            "use crate::util::helper;\nfn run() { helper(); }\n",
            &[("src/util.rs", "pub use crate::internal::helper;\n")],
        ),
        &[link(
            0,
            0,
            "crate::util::helper",
            // TASK 3B names the re-export declaration (a `use` item), which is
            // not a `function`.
            LinkOutcome::exact(LinkTarget::declaration(
                "src/util.rs",
                0,
                "crate::util",
                "import",
                "helper",
            )),
        )],
    );
    assert_no_candidate(&records, "helper", 0, "import_target_not_eligible_function");
}

#[test]
fn provenance_identifies_the_call_import_link_and_target() {
    let records = rule_rust::candidates(
        &analyses(
            "use crate::util::helper;\nfn run() { helper(); }\n",
            &[(TARGET, "pub fn helper() {}\n")],
        ),
        &[link(
            0,
            0,
            "crate::util::helper",
            LinkOutcome::exact(fn_target(TARGET, 0, "helper")),
        )],
    );
    let record = calls_to(&records, "helper")[0];
    let ev = evidence(record);
    assert!(ev.contains("blocker=import"), "{ev}");
    assert!(ev.contains("blocker_import_id=0"), "{ev}");
    assert!(ev.contains("blocker_local_name=helper"), "{ev}");
    assert!(ev.contains("import_link_id="), "{ev}");
    assert!(ev.contains("link_outcome=exact"), "{ev}");
    assert!(ev.contains("import_written=crate::util::helper"), "{ev}");
    assert_eq!(
        record.rule_id,
        candidate_rule::RUST_CALL_IMPORTED_FUNCTION_CANDIDATE
    );
}
