//! PHP bounded receiver-type evidence V1 (PHP_BOUNDED_RECEIVER_TYPE_EVIDENCE_V1).
//!
//! Source-grounded assertions over the frozen `fixtures/phprx` corpus. Every
//! receiver-evidence shape from the milestone contract is asserted: typed
//! receivers gain bounded method candidates; unsupported/dynamic/rewritten
//! receivers stay honest `OutOfScope`/`NoCandidate`.

use std::collections::BTreeSet;

mod support;

use repodex::candidates::{
    build_candidates, candidate_rule, CallCandidateRecord, CandidateIndex, CandidateOutcome,
};
use repodex::links::build_links;
use repodex::model::ReceiverEvidenceKind;
use repodex::repository::{build_snapshot, BuildOptions};
use support::TempDir;

const TYPED: &str = candidate_rule::PHP_CALL_TYPED_RECEIVER_METHOD_CANDIDATE;
const PARAMS: &str = "src/ParamCalls.php";
const PROPS: &str = "src/PropCalls.php";
const CONTROLS: &str = "src/Controls.php";

fn candidates_index(temp: &TempDir) -> CandidateIndex {
    let root = support::fixture("phprx");
    let snap = temp.path().join("snap");
    build_snapshot(&support::analyzer(), &root, &snap, BuildOptions::default())
        .expect("snapshot build");
    let links = temp.path().join("links");
    build_links(&snap, Some(&root), &links).expect("link build");
    let cand = temp.path().join("cand");
    build_candidates(&snap, &links, &cand).expect("candidate build");
    CandidateIndex::load(&cand).expect("load candidates")
}

/// Records are per call occurrence; `written` alone is not unique, so locate
/// by enclosing declaration name too.
fn record_in<'a>(
    index: &'a CandidateIndex,
    path: &str,
    method: &str,
    written: &str,
) -> &'a CallCandidateRecord {
    index
        .records()
        .iter()
        .filter(|r| r.source.relative_path == path && r.written == written)
        .find(|r| r.provenance.scope_path.ends_with(&format!("::{method}")))
        .unwrap_or_else(|| panic!("no record for {path}::{method} `{written}`"))
}

fn targets(record: &CallCandidateRecord) -> BTreeSet<String> {
    record
        .outcome
        .candidates()
        .iter()
        .map(|c| {
            format!(
                "{}#{}:{}:{}",
                c.relative_path, c.declaration_id, c.declaration_kind, c.name
            )
        })
        .collect()
}

fn assert_call(
    index: &CandidateIndex,
    path: &str,
    method: &str,
    written: &str,
    outcome: &str,
    candidates: &[&str],
) {
    let r = record_in(index, path, method, written);
    assert_eq!(r.rule_id, TYPED, "{method} `{written}` rule");
    assert_eq!(r.outcome.as_str(), outcome, "{method} `{written}` outcome");
    let expected: BTreeSet<String> = candidates.iter().map(|s| s.to_string()).collect();
    assert_eq!(targets(r), expected, "{method} `{written}` candidates");
}

#[test]
fn parameter_type_hint_direct_aliased_fq_nullable() {
    let t = TempDir::new("phprx");
    let i = candidates_index(&t);
    assert_call(
        &i,
        PARAMS,
        "direct",
        "$service->handle",
        "single_candidate",
        &["src/Types.php#8:method:handle"],
    );
    assert_call(
        &i,
        PARAMS,
        "aliased",
        "$s->run",
        "single_candidate",
        &["src/Types.php#7:method:run"],
    );
    assert_call(
        &i,
        PARAMS,
        "qualified",
        "$r->save",
        "single_candidate",
        &["src/Types.php#2:method:save"],
    );
    // `?Cache` -> Cache; nullsafe marker preserved on the call.
    let r = record_in(&i, PARAMS, "nullable", "$c?->get");
    assert_eq!(r.outcome.as_str(), "single_candidate");
}

#[test]
fn union_arms_resolve_per_method_intersection_unsupported() {
    let t = TempDir::new("phprx");
    let i = candidates_index(&t);
    // `Service|Cache` -> both arms; each call resolves the arm's method.
    assert_call(
        &i,
        PARAMS,
        "unioned",
        "$x->run",
        "single_candidate",
        &["src/Types.php#7:method:run"],
    );
    assert_call(
        &i,
        PARAMS,
        "unioned",
        "$x->get",
        "single_candidate",
        &["src/Types.php#5:method:get"],
    );
    // `Repo&Cache` intersection -> no candidate (V1 boundary).
    assert_call(&i, PARAMS, "intersected", "$x->save", "no_candidate", &[]);
}

#[test]
fn builtin_parent_external_and_duplicate_hints() {
    let t = TempDir::new("phprx");
    let i = candidates_index(&t);
    // int -> filtered builtin, no class candidate.
    assert_call(&i, PARAMS, "builtin", "$n->abs", "no_candidate", &[]);
    // parent -> unsupported arm, no candidate.
    assert_call(
        &i,
        PARAMS,
        "parentTyped",
        "$p->anything",
        "no_candidate",
        &[],
    );
    // Vendor\Pkg\Tool -> unindexed external name.
    assert_call(&i, PARAMS, "external", "$t->work", "no_candidate", &[]);
    // duplicate class decls -> both preserved.
    assert_call(
        &i,
        PARAMS,
        "duplicated",
        "$t->m",
        "multiple_candidates",
        &["src/Dup/A.php#2:method:m", "src/Dup/B.php#2:method:m"],
    );
}

#[test]
fn self_type_maps_to_lexical_class() {
    let t = TempDir::new("phprx");
    let i = candidates_index(&t);
    assert_call(
        &i,
        PARAMS,
        "selfTyped",
        "$s->helper",
        "single_candidate",
        &["src/ParamCalls.php#18:method:helper"],
    );
}

#[test]
fn parameter_reassignment_and_write_rules() {
    let t = TempDir::new("phprx");
    let i = candidates_index(&t);
    // `Service $x; $x = dynMake(); $x->run()` — opaque write kills the hint.
    let r = record_in(&i, PARAMS, "overwritten", "$x->run");
    assert_eq!(r.outcome.as_str(), "out_of_scope");
    assert!(
        matches!(&r.outcome, CandidateOutcome::OutOfScope { reason } if reason == "receiver_type_unavailable")
    );
    // `Service $x; $x = new Alt(); $x->go()` — literal-new write wins.
    assert_call(
        &i,
        PARAMS,
        "reassigned",
        "$x->go",
        "single_candidate",
        &["src/Types.php#14:method:go"],
    );
    // sequential literal-news union conservatively (no CFG).
    assert_call(
        &i,
        PARAMS,
        "sequential",
        "$x->go",
        "multiple_candidates",
        &["src/Types.php#12:method:go", "src/Types.php#14:method:go"],
    );
    // branch writes -> both candidates, never one picked.
    assert_call(
        &i,
        PARAMS,
        "branched",
        "$x->go",
        "multiple_candidates",
        &["src/Types.php#12:method:go", "src/Types.php#14:method:go"],
    );
    // `new Local(); $x = weird(); $x->go()` — opaque nearest write.
    let r = record_in(&i, PARAMS, "lateOpaque", "$x->go");
    assert_eq!(r.outcome.as_str(), "out_of_scope");
}

#[test]
fn property_type_hints_promoted_and_nullable() {
    let t = TempDir::new("phprx");
    let i = candidates_index(&t);
    assert_call(
        &i,
        PROPS,
        "read",
        "$this->svc->handle",
        "single_candidate",
        &["src/Types.php#8:method:handle"],
    );
    assert_call(
        &i,
        PROPS,
        "read",
        "$this->cache->get",
        "single_candidate",
        &["src/Types.php#5:method:get"],
    );
    assert_call(
        &i,
        PROPS,
        "read",
        "$this->maybe?->save",
        "single_candidate",
        &["src/Types.php#2:method:save"],
    );
}

#[test]
fn property_literal_new_and_multiple_assignments() {
    let t = TempDir::new("phprx");
    let i = candidates_index(&t);
    // `$this->pages = new Pages()` in the ctor, called from another method.
    assert_call(
        &i,
        PROPS,
        "home",
        "$this->pages->render",
        "single_candidate",
        &["src/Types.php#10:method:render"],
    );
    // untyped prop written `new Pages()` in another method -> still evidence.
    assert_call(
        &i,
        PROPS,
        "read",
        "$this->plain->render",
        "single_candidate",
        &["src/Types.php#10:method:render"],
    );
    // two literal-new writes to `$this->dup` -> multiple candidates.
    assert_call(
        &i,
        PROPS,
        "read",
        "$this->dup->go",
        "multiple_candidates",
        &["src/Types.php#12:method:go", "src/Types.php#14:method:go"],
    );
    // no evidence at all -> OutOfScope; typed prop, missing method -> none.
    let r = record_in(&i, PROPS, "untouched", "$this->missing->run");
    assert_eq!(r.outcome.as_str(), "out_of_scope");
    let r = record_in(&i, PROPS, "untouched", "$this->svc->absent");
    assert_eq!(r.outcome.as_str(), "no_candidate");
}

#[test]
fn controls_stay_honest() {
    let t = TempDir::new("phprx");
    let i = candidates_index(&t);
    // untyped `$service` — no evidence.
    let r = record_in(&i, CONTROLS, "unknown", "$service->handle");
    assert_eq!(r.outcome.as_str(), "out_of_scope");
    // dynamic member name — not affected by the typed rule.
    let r = record_in(&i, CONTROLS, "dynamic", "$s->{$m}");
    assert_eq!(r.outcome.as_str(), "out_of_scope");
    assert_eq!(
        r.rule_id,
        candidate_rule::PHP_CALL_LEXICAL_THIS_METHOD_CANDIDATE
    );
    // chained: `$s->factory` typed but missing method; `()->build` opaque.
    let r = record_in(&i, CONTROLS, "chained", "$s->factory");
    assert_eq!(r.outcome.as_str(), "no_candidate");
    let r = record_in(&i, CONTROLS, "chained", "$s->factory()->build");
    assert_eq!(r.outcome.as_str(), "out_of_scope");
    // arrow-function parameter hint (anonymous scope under `arrow`).
    let r = record_in(&i, CONTROLS, "arrow::(arrow_function)", "$s->run");
    assert_eq!(r.outcome.as_str(), "single_candidate");
    assert!(targets(r).contains("src/Types.php#7:method:run"));
}

#[test]
fn evidence_facts_are_normalized() {
    // §29: written type hints exist as facts even when the class cannot be
    // resolved (Vendor\Pkg\Tool) or is a builtin (int).
    let a = support::analyze_fixture("phprx/src/ParamCalls.php");
    let ev = &a.receiver_type_evidence;
    let kinds: BTreeSet<_> = ev.iter().map(|e| e.kind).collect();
    assert!(kinds.contains(&ReceiverEvidenceKind::ParameterTypeHint));
    assert!(ev.iter().any(|e| e.written == "Service|Cache"));
    assert!(ev.iter().any(|e| e.written == "Repo&Cache"));
    assert!(ev.iter().any(|e| e.written == "int"));
    assert!(ev.iter().any(|e| e.written == "Tool"));
    assert!(ev
        .iter()
        .any(|e| e.kind == ReceiverEvidenceKind::LocalLiteralNew && e.written == "Alt"));
    assert!(ev
        .iter()
        .any(|e| e.kind == ReceiverEvidenceKind::LocalOpaqueWrite));
}
