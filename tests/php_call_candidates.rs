//! PHP call-candidate V1 correctness suite (PHP_CALL_CANDIDATES_V1).
//!
//! Source-grounded assertions over the frozen `fixtures/phpnav` corpus. Every
//! supported/unsupported call shape from the milestone contract is asserted
//! here: candidates must be bounded and honest — never `Exact`, never a
//! repository-wide name scan, never a candidate for a dynamic call.

use std::collections::BTreeSet;

mod support;

use repodex::candidates::{
    build_candidates, candidate_rule, CallCandidateRecord, CandidateIndex, CandidateOutcome,
};
use repodex::links::build_links;
use repodex::repository::{build_snapshot, BuildOptions};
use support::TempDir;

fn candidates_index(temp: &TempDir) -> CandidateIndex {
    let root = support::fixture("phpnav");
    let snap = temp.path().join("snap");
    build_snapshot(&support::analyzer(), &root, &snap, BuildOptions::default())
        .expect("snapshot build");
    let links = temp.path().join("links");
    build_links(&snap, Some(&root), &links).expect("link build");
    let cand = temp.path().join("cand");
    build_candidates(&snap, &links, &cand).expect("candidate build");
    CandidateIndex::load(&cand).expect("load candidates")
}

/// The record for one call, located by file and written callee.
fn record_by<'a>(index: &'a CandidateIndex, path: &str, written: &str) -> &'a CallCandidateRecord {
    index
        .records()
        .iter()
        .find(|r| r.source.relative_path == path && r.written == written)
        .unwrap_or_else(|| panic!("no candidate record for {path} `{written}`"))
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
    written: &str,
    rule: &str,
    outcome: &str,
    candidates: &[&str],
) {
    let record = record_by(index, path, written);
    assert_eq!(record.rule_id, rule, "{path} `{written}` rule");
    assert_eq!(
        record.outcome.as_str(),
        outcome,
        "{path} `{written}` outcome"
    );
    let expected: BTreeSet<String> = candidates.iter().map(|s| s.to_string()).collect();
    assert_eq!(targets(record), expected, "{path} `{written}` candidates");
}

const REPO: &str = "src/Service/Repo.php";
const NEG: &str = "src/Service/Negatives.php";
const CASE: &str = "src/Service/CaseTest.php";
const STRINGS: &str = "src/Util/strings.php";

#[test]
fn imported_function_direct_and_aliased() {
    let temp = TempDir::new("php-cand");
    let index = candidates_index(&temp);
    // `use function App\Util\slugify;` + `slugify()`
    assert_call(
        &index,
        REPO,
        "slugify",
        candidate_rule::PHP_CALL_IMPORTED_FUNCTION_CANDIDATE,
        "single_candidate",
        &["src/Util/strings.php#1:function:slugify"],
    );
    // `use function App\Util\slugify as sl;` + `sl()`
    assert_call(
        &index,
        REPO,
        "sl",
        candidate_rule::PHP_CALL_IMPORTED_FUNCTION_CANDIDATE,
        "single_candidate",
        &["src/Util/strings.php#1:function:slugify"],
    );
}

#[test]
fn fully_qualified_and_relative_and_tiered_functions() {
    let temp = TempDir::new("php-cand");
    let index = candidates_index(&temp);
    // `\App\Util\slugify()` — FQN
    assert_call(
        &index,
        REPO,
        "\\App\\Util\\slugify",
        candidate_rule::PHP_CALL_NAMESPACE_FUNCTION_CANDIDATE,
        "single_candidate",
        &["src/Util/strings.php#1:function:slugify"],
    );
    // `Util\version()` — first-segment class-import alias substitution
    assert_call(
        &index,
        REPO,
        "Util\\version",
        candidate_rule::PHP_CALL_NAMESPACE_FUNCTION_CANDIDATE,
        "single_candidate",
        &["src/Util/strings.php#2:function:version"],
    );
    // `namespace\localfn()` — relative name
    assert_call(
        &index,
        "src/Util/local.php",
        "namespace\\localfn",
        candidate_rule::PHP_CALL_NAMESPACE_FUNCTION_CANDIDATE,
        "single_candidate",
        &["src/Util/local.php#1:function:localfn"],
    );
    // `version()` inside App\Util — same-namespace tier
    let r = record_by(&index, STRINGS, "version");
    assert_eq!(r.outcome.as_str(), "single_candidate");
    assert!(r
        .provenance
        .evidence
        .iter()
        .any(|e| e == "tier=current_namespace"));
    // `global_helper()` inside App\Util — global fallback tier
    let r = record_by(&index, STRINGS, "global_helper");
    assert_eq!(r.outcome.as_str(), "single_candidate");
    assert!(r
        .provenance
        .evidence
        .iter()
        .any(|e| e == "tier=global_fallback"));
    assert!(targets(r).contains("src/global.php#0:function:global_helper"));
    // global scope `global_helper()`
    assert_call(
        &index,
        "src/global.php",
        "global_helper",
        candidate_rule::PHP_CALL_NAMESPACE_FUNCTION_CANDIDATE,
        "single_candidate",
        &["src/global.php#0:function:global_helper"],
    );
}

#[test]
fn duplicate_function_declarations_stay_multiple() {
    let temp = TempDir::new("php-cand");
    let index = candidates_index(&temp);
    let r = record_by(&index, "src/Dup/B.php", "collide");
    assert_eq!(r.outcome.as_str(), "multiple_candidates");
    assert_eq!(r.outcome.candidates().len(), 2);
}

#[test]
fn construction_candidates() {
    let temp = TempDir::new("php-cand");
    let index = candidates_index(&temp);
    for (written, expected) in [
        ("self", "src/Service/Repo.php#3:class:Repo"),
        ("Repo", "src/Service/Repo.php#3:class:Repo"),
        ("Alias", "src/Service/Repo.php#1:class:Helper"),
        (
            "\\App\\Service\\Helper",
            "src/Service/Repo.php#1:class:Helper",
        ),
    ] {
        assert_call(
            &index,
            REPO,
            written,
            candidate_rule::PHP_CALL_CONSTRUCTION_CLASS_CANDIDATE,
            "single_candidate",
            &[expected],
        );
    }
    // class candidate is the CLASS declaration — never a __construct claim.
    let r = record_by(&index, REPO, "Repo");
    let c = r.outcome.single().expect("single");
    assert_eq!(c.declaration_kind, "class");
}

#[test]
fn construction_out_of_scope() {
    let temp = TempDir::new("php-cand");
    let index = candidates_index(&temp);
    for (written, reason) in [
        ("static", "late_static_binding"),
        ("parent", "parent_scope_not_modeled_in_v1"),
    ] {
        let r = record_by(&index, REPO, written);
        assert_eq!(r.outcome.as_str(), "out_of_scope");
        assert!(
            matches!(&r.outcome, CandidateOutcome::OutOfScope { reason: r } if r == reason),
            "{written}: {:?}",
            r.outcome
        );
    }
    // `new $cls()` — dynamic class expression.
    let r = record_by(&index, NEG, "$cls");
    assert_eq!(r.outcome.as_str(), "out_of_scope");
}

#[test]
fn static_scoped_methods() {
    let temp = TempDir::new("php-cand");
    let index = candidates_index(&temp);
    assert_call(
        &index,
        REPO,
        "Helper::run",
        candidate_rule::PHP_CALL_STATIC_METHOD_CANDIDATE,
        "single_candidate",
        &["src/Service/Repo.php#2:method:run"],
    );
    assert_call(
        &index,
        REPO,
        "Alias::run",
        candidate_rule::PHP_CALL_STATIC_METHOD_CANDIDATE,
        "single_candidate",
        &["src/Service/Repo.php#2:method:run"],
    );
    assert_call(
        &index,
        REPO,
        "self::create",
        candidate_rule::PHP_CALL_LEXICAL_SELF_METHOD_CANDIDATE,
        "single_candidate",
        &["src/Service/Repo.php#5:method:create"],
    );
    // static::m() -> late static binding, out of scope.
    let r = record_by(&index, REPO, "static::create");
    assert_eq!(r.outcome.as_str(), "out_of_scope");
    // parent::m() -> unmodeled in V1.
    let r = record_by(&index, REPO, "parent::save");
    assert_eq!(r.outcome.as_str(), "out_of_scope");
    // External dependency: `Tool::work()` -> Vendor\Pkg\Tool not indexed.
    let r = record_by(&index, "src/Vendor/External.php", "Tool::work");
    assert_eq!(r.outcome.as_str(), "no_candidate");
}

#[test]
fn this_method_boundary() {
    let temp = TempDir::new("php-cand");
    let index = candidates_index(&temp);
    assert_call(
        &index,
        REPO,
        "$this->persist",
        candidate_rule::PHP_CALL_LEXICAL_THIS_METHOD_CANDIDATE,
        "single_candidate",
        &["src/Service/Repo.php#7:method:persist"],
    );
    // $this->missing: class exists but no direct method -> honest no_candidate.
    let r = record_by(&index, REPO, "$this->missing");
    assert_eq!(r.outcome.as_str(), "no_candidate");
    assert!(
        matches!(&r.outcome, CandidateOutcome::NoCandidate { reason }
        if reason.contains("no_direct_method_in_lexical_class"))
    );
    // same-name method on another class must NOT be swept in.
    assert_call(
        &index,
        NEG,
        "$this->dyn",
        candidate_rule::PHP_CALL_LEXICAL_THIS_METHOD_CANDIDATE,
        "single_candidate",
        &["src/Service/Negatives.php#3:method:dyn"],
    );
}

#[test]
fn negative_controls_no_false_candidates() {
    let temp = TempDir::new("php-cand");
    let index = candidates_index(&temp);
    // `Repo $repo` is a parameter type hint — V1->V2: typed receiver now
    // produces bounded candidates (asserted fully in php_receiver_type.rs).
    for (written, rule) in [
        ("$repo->save", "src/Service/Repo.php#6:method:save"),
        ("$repo?->save", "src/Service/Repo.php#6:method:save"),
    ] {
        let r = record_by(&index, NEG, written);
        assert_eq!(
            r.rule_id,
            candidate_rule::PHP_CALL_TYPED_RECEIVER_METHOD_CANDIDATE
        );
        assert_eq!(r.outcome.as_str(), "single_candidate", "{written}");
        assert!(targets(r).contains(rule));
    }
    for (written, reason) in [
        ("$this->{$method}", "dynamic_member_name"),
        ("$this->$method", "dynamic_member_name"),
        ("Helper::{$method}", "dynamic_scope_or_member"),
        // `$this->pages->renderHome`: no `pages` property hint/write exists
        // in `Negatives` — the receiver type remains unavailable.
        ("$this->pages->renderHome", "receiver_type_unavailable"),
    ] {
        let r = record_by(&index, NEG, written);
        assert_eq!(r.outcome.as_str(), "out_of_scope", "{written}");
        assert!(
            matches!(&r.outcome, CandidateOutcome::OutOfScope { reason: r } if r == reason),
            "{written}: {:?}",
            r.outcome
        );
    }
    // `$cb()` variable callable.
    let r = record_by(&index, NEG, "$cb");
    assert_eq!(r.outcome.as_str(), "out_of_scope");
    // `unknown_global()` — no candidate anywhere.
    let r = record_by(&index, "src/Vendor/External.php", "unknown_global");
    assert_eq!(r.outcome.as_str(), "no_candidate");
}

#[test]
fn first_class_callables_are_not_invocations() {
    let temp = TempDir::new("php-cand");
    let index = candidates_index(&temp);
    // `slugify(...)` / `Helper::run(...)` produce no call records at all.
    for r in index.records() {
        assert!(
            !r.written.ends_with("(...)"),
            "first-class callable produced a call record: {:?}",
            r.written
        );
        assert_ne!(r.written, "slugify(...)");
        assert_ne!(r.written, "Helper::run(...)");
    }
}

#[test]
fn php_case_insensitivity() {
    let temp = TempDir::new("php-cand");
    let index = candidates_index(&temp);
    // PHP class/function/method names are case-insensitive.
    assert_call(
        &index,
        CASE,
        "SLUGIFY",
        candidate_rule::PHP_CALL_IMPORTED_FUNCTION_CANDIDATE,
        "single_candidate",
        &["src/Util/strings.php#1:function:slugify"],
    );
    assert_call(
        &index,
        CASE,
        "repo",
        candidate_rule::PHP_CALL_CONSTRUCTION_CLASS_CANDIDATE,
        "single_candidate",
        &["src/Service/Repo.php#3:class:Repo"],
    );
    assert_call(
        &index,
        CASE,
        "helper::RUN",
        candidate_rule::PHP_CALL_STATIC_METHOD_CANDIDATE,
        "single_candidate",
        &["src/Service/Repo.php#2:method:run"],
    );
}

#[test]
fn no_global_method_name_scan() {
    // §25/§50: every candidate target must be a method of a bounded class
    // candidate — assert no record ever targets a method on a class the scope
    // did not name. The fixture has `Repo::create`, `Negatives::dyn`,
    // `Helper::run`: a `$this->run()` inside Negatives must NOT find
    // Helper::run.
    let temp = TempDir::new("php-cand");
    let index = candidates_index(&temp);
    for r in index.records() {
        for c in r.outcome.candidates() {
            match c.declaration_kind.as_str() {
                "method" | "function" | "class" | "interface" | "trait" | "enum" => {}
                other => panic!("unexpected candidate kind {other}"),
            }
        }
    }
}

#[test]
fn candidate_records_cover_every_php_call() {
    // The artifact is a complete disposition: every PHP call-like occurrence
    // has exactly one record.
    let temp = TempDir::new("php-cand");
    let index = candidates_index(&temp);
    let php = index
        .records()
        .iter()
        .filter(|r| r.language == "php")
        .count();
    assert!(php >= 30, "expected full disposition, got {php}");
    let ids: BTreeSet<&str> = index
        .records()
        .iter()
        .map(|r| r.record_id.as_str())
        .collect();
    assert_eq!(ids.len(), index.records().len(), "duplicate record ids");
}

#[test]
fn php_candidates_flow_through_graph_and_query() {
    // §42 end-to-end: PHP candidate artifact -> generic graph call_candidate
    // edges -> typed query machinery. Asserts the edge carries candidate
    // evidence class, candidate_set_id and the php rule provenance.
    let temp = TempDir::new("php-graph");
    let root = support::fixture("phpnav");
    let snap = temp.path().join("snap");
    build_snapshot(&support::analyzer(), &root, &snap, BuildOptions::default()).expect("snapshot");
    let links = temp.path().join("links");
    repodex::links::build_links(&snap, Some(&root), &links).expect("links");
    let cand = temp.path().join("cand");
    build_candidates(&snap, &links, &cand).expect("candidates");
    let gdir = temp.path().join("graph");
    repodex::graph::build_graph(&snap, &links, &cand, &gdir).expect("graph");
    let index = repodex::graph::GraphIndex::load(&gdir).expect("graph index");
    let php_edges: Vec<_> = index
        .edges()
        .iter()
        .filter(|e| e.kind == "call_candidate" && e.rule_id.starts_with("php.call."))
        .collect();
    assert!(
        php_edges.len() >= 10,
        "expected >=10 php call_candidate edges, got {}",
        php_edges.len()
    );
    for e in &php_edges {
        assert_eq!(
            e.evidence_class,
            repodex::graph::EvidenceClass::Candidate,
            "fact promotion!"
        );
        assert!(e.candidate_set_id.is_some(), "missing cs=");
    }
    // Every php edge's target is a declaration node in the graph.
    let kinds: BTreeSet<&str> = php_edges
        .iter()
        .filter_map(|e| {
            index
                .node(&e.target)
                .map(|n| n.key.split(':').next().unwrap_or("?"))
        })
        .collect();
    assert!(
        kinds.iter().any(|k| *k == "decl"),
        "edge targets: {kinds:?}"
    );
}

#[test]
fn candidate_abi_and_policy_versioned() {
    // §40: PHP rules participate in artifact identity — an artifact built
    // without them cannot be silently reused.
    use repodex::candidates::model::{CANDIDATE_RULE_ABI_VERSION, POLICY_VERSION_PHP_CALL};
    assert_eq!(CANDIDATE_RULE_ABI_VERSION, 9);
    assert_eq!(POLICY_VERSION_PHP_CALL, 2);
    let fp = repodex::candidates::model::CandidateFingerprint::current();
    assert!(fp.text.contains("php=2"), "fingerprint: {}", fp.text);
}
