//! PHP bounded inheritance dispatch V1 (PHP_BOUNDED_INHERITANCE_DISPATCH_V1).
//!
//! Source-grounded assertions over the frozen `fixtures/phpinh` corpus:
//! `php.class.extends` links feed a cycle-safe nearest-level ancestor method
//! walk for `$this`, `self::`, scoped names, typed receivers and `parent::`.
//! Visibility, override precedence, ambiguity and the unsupported boundary
//! (traits, interfaces, late static) are all asserted.

use std::collections::BTreeSet;

mod support;

use repodex::candidates::{
    build_candidates, candidate_rule, CallCandidateRecord, CandidateIndex, CandidateOutcome,
};
use repodex::links::{build_links, model::rule, LinkIndex};
use repodex::repository::{build_snapshot, BuildOptions};
use support::TempDir;

const INHERITED: &str = candidate_rule::PHP_CALL_INHERITED_METHOD_CANDIDATE;
const PARENT_M: &str = candidate_rule::PHP_CALL_PARENT_METHOD_CANDIDATE;
const PARENT_C: &str = candidate_rule::PHP_CALL_PARENT_CONSTRUCTION_CANDIDATE;

fn build(temp: &TempDir) -> (CandidateIndex, LinkIndex) {
    let root = support::fixture("phpinh");
    let snap = temp.path().join("snap");
    build_snapshot(&support::analyzer(), &root, &snap, BuildOptions::default())
        .expect("snapshot build");
    let links = temp.path().join("links");
    build_links(&snap, Some(&root), &links).expect("link build");
    let cand = temp.path().join("cand");
    build_candidates(&snap, &links, &cand).expect("candidate build");
    (
        CandidateIndex::load(&cand).expect("load candidates"),
        LinkIndex::load(&links).expect("load links"),
    )
}

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

fn target_methods(record: &CallCandidateRecord) -> BTreeSet<String> {
    record
        .outcome
        .candidates()
        .iter()
        .map(|c| c.scope_path.clone() + "::" + &c.name)
        .collect()
}

fn has_target(record: &CallCandidateRecord, class: &str, method: &str) -> bool {
    target_methods(record)
        .iter()
        .any(|t| t.ends_with(&format!("::{class}::{method}")))
}

#[test]
fn hierarchy_links_are_emitted_for_every_clause() {
    let temp = TempDir::new("phpinh");
    let (_cand, links) = build(&temp);
    let extends: Vec<_> = links
        .links()
        .iter()
        .filter(|l| l.rule_id == rule::PHP_CLASS_EXTENDS)
        .collect();
    let implements: Vec<_> = links
        .links()
        .iter()
        .filter(|l| l.rule_id == rule::PHP_CLASS_IMPLEMENTS)
        .collect();
    let traits: Vec<_> = links
        .links()
        .iter()
        .filter(|l| l.rule_id == rule::PHP_CLASS_USES_TRAIT)
        .collect();
    assert!(extends.len() >= 12, "extends links: {}", extends.len());
    assert_eq!(implements.len(), 1, "Impl implements ServiceContract");
    assert_eq!(traits.len(), 1, "WithTrait uses TLogger");
    // Duplicate parent declaration stays Ambiguous; external stays Unresolved.
    let dup = extends
        .iter()
        .filter(|l| l.written == "SharedBase")
        .collect::<Vec<_>>();
    assert_eq!(dup.len(), 1);
    assert!(
        matches!(
            dup[0].outcome,
            repodex::links::model::LinkOutcome::Ambiguous { .. }
        ),
        "SharedBase must stay ambiguous"
    );
    let ext = extends
        .iter()
        .filter(|l| l.written == "Vendor\\Missing")
        .collect::<Vec<_>>();
    assert_eq!(ext.len(), 1);
    assert!(
        matches!(
            ext[0].outcome,
            repodex::links::model::LinkOutcome::Unresolved { .. }
        ),
        "Vendor\\Missing must be unresolved"
    );
}

#[test]
fn inherited_method_lookup_uses_nearest_level() {
    let temp = TempDir::new("phpinh");
    let (index, _) = build(&temp);
    // D::use_it -> A::base through B/C (depth 3).
    let base = record_in(&index, "src/Hierarchy.php", "use_it", "$this->base");
    assert_eq!(base.rule_id, INHERITED);
    assert!(matches!(
        base.outcome,
        CandidateOutcome::SingleCandidate { .. }
    ));
    assert!(has_target(base, "A", "base"));
    // D::use_it -> D::over (own override wins; A::over must NOT appear).
    let over = record_in(&index, "src/Hierarchy.php", "use_it", "$this->over");
    assert_eq!(
        over.rule_id,
        candidate_rule::PHP_CALL_LEXICAL_THIS_METHOD_CANDIDATE
    );
    let t = target_methods(over);
    assert!(t.iter().any(|m| m.ends_with("::D::over")));
    assert!(!t.iter().any(|m| m.ends_with("::A::over")), "{t:?}");
    assert!(!t.iter().any(|m| m.ends_with("::B::over")), "{t:?}");
    // D::use_it -> B::only_b (level 2).
    let only_b = record_in(&index, "src/Hierarchy.php", "use_it", "$this->only_b");
    assert_eq!(only_b.rule_id, INHERITED);
    assert!(has_target(only_b, "B", "only_b"));
    // ancestor_depth in provenance evidence.
    assert!(
        only_b
            .provenance
            .evidence
            .iter()
            .any(|e| e.starts_with("ancestor_depth=")),
        "{:?}",
        only_b.provenance.evidence
    );
}

#[test]
fn visibility_and_missing_boundaries() {
    let temp = TempDir::new("phpinh");
    let (index, _) = build(&temp);
    // Protected ancestor method is callable.
    let prot = record_in(&index, "src/Hierarchy.php", "use_it", "$this->prot");
    assert!(has_target(prot, "A", "prot"));
    // Private ancestor method is never callable -> no candidate.
    let hidden = record_in(&index, "src/Hierarchy.php", "use_it", "$this->hidden");
    assert!(matches!(
        hidden.outcome,
        CandidateOutcome::NoCandidate { .. }
    ));
    // Absent everywhere -> no candidate, never guessed.
    let missing = record_in(&index, "src/Hierarchy.php", "use_it", "$this->missing");
    assert!(matches!(
        missing.outcome,
        CandidateOutcome::NoCandidate { .. }
    ));
    // Typed receiver on a private ancestor method -> no candidate either.
    let thidden = record_in(&index, "src/Typed.php", "f", "$a->hidden");
    assert!(matches!(
        thidden.outcome,
        CandidateOutcome::NoCandidate { .. }
    ));
}

#[test]
fn self_and_parent_scope_dispatch() {
    let temp = TempDir::new("phpinh");
    let (index, _) = build(&temp);
    // self::prot -> A::prot via inheritance.
    let selfp = record_in(&index, "src/Hierarchy.php", "use_it", "self::prot");
    assert_eq!(selfp.rule_id, INHERITED);
    assert!(has_target(selfp, "A", "prot"));
    // parent::over -> B::over (C has none; the walk starts at C).
    let parent = record_in(&index, "src/Hierarchy.php", "use_it", "parent::over");
    assert_eq!(parent.rule_id, PARENT_M);
    assert!(has_target(parent, "B", "over"));
    // new parent() -> class C (not __construct resolution).
    let newp = record_in(&index, "src/Hierarchy.php", "use_it", "parent");
    assert_eq!(newp.rule_id, PARENT_C);
    assert!(newp
        .outcome
        .candidates()
        .iter()
        .any(|c| c.name == "C" && c.declaration_kind == "class"));
}

#[test]
fn late_static_and_trait_dispatch_stay_out_of_scope() {
    let temp = TempDir::new("phpinh");
    let (index, _) = build(&temp);
    let st = record_in(&index, "src/Hierarchy.php", "use_it", "static::later");
    assert!(matches!(st.outcome, CandidateOutcome::OutOfScope { .. }));
    let ns = record_in(&index, "src/Hierarchy.php", "use_it", "static");
    assert!(matches!(ns.outcome, CandidateOutcome::OutOfScope { .. }));
    // Trait method dispatch is not part of V1 -> honest no_candidate.
    let trait_m = record_in(&index, "src/Contracts.php", "g", "$this->logline");
    assert!(matches!(
        trait_m.outcome,
        CandidateOutcome::NoCandidate { .. }
    ));
}

#[test]
fn typed_receiver_walks_the_declared_hierarchy() {
    let temp = TempDir::new("phpinh");
    let (index, _) = build(&temp);
    // Direct method keeps the typed-receiver rule.
    let direct = record_in(&index, "src/Typed.php", "f", "$a->base");
    assert_eq!(
        direct.rule_id,
        candidate_rule::PHP_CALL_TYPED_RECEIVER_METHOD_CANDIDATE
    );
    assert!(has_target(direct, "A", "base"));
    // Inherited method reports the inherited rule and keeps receiver provenance.
    let inh = record_in(&index, "src/Typed.php", "f", "$b->base");
    assert_eq!(inh.rule_id, INHERITED);
    assert!(has_target(inh, "A", "base"));
    assert!(inh
        .provenance
        .evidence
        .iter()
        .any(|e| e.starts_with("receiver=")));
    // Local literal-new receiver follows the same walk.
    let local = record_in(&index, "src/Typed.php", "mk", "$x->base");
    assert_eq!(local.rule_id, INHERITED);
    assert!(has_target(local, "A", "base"));
    // `parent` type hint binds through the extends link.
    let ph = record_in(&index, "src/Contracts.php", "f", "$p->mark");
    assert!(matches!(
        ph.outcome,
        CandidateOutcome::SingleCandidate { .. }
    ));
    assert!(has_target(ph, "ParentHint", "mark"));
}

#[test]
fn duplicate_and_external_and_cycle_parents_are_safe() {
    let temp = TempDir::new("phpinh");
    let (index, _) = build(&temp);
    // Each ambiguous arm contributes its own method; none collapse.
    let s1 = record_in(&index, "src/DupChild.php", "f", "$this->shared_one");
    let s2 = record_in(&index, "src/DupChild.php", "f", "$this->shared_two");
    assert!(s1
        .outcome
        .candidates()
        .iter()
        .any(|c| c.relative_path.ends_with("P1.php")));
    assert!(s2
        .outcome
        .candidates()
        .iter()
        .any(|c| c.relative_path.ends_with("P2.php")));
    // External parent -> no candidates, no scan.
    let ext = record_in(&index, "src/DupChild.php", "f", "$this->anything");
    assert!(matches!(ext.outcome, CandidateOutcome::NoCandidate { .. }));
    // Cycle: terminates, still finds the method reachable through the cycle.
    let cyc = record_in(&index, "src/Cycle.php", "f", "$this->c2m");
    assert!(has_target(cyc, "Cyc2", "c2m"));
    let missing = record_in(&index, "src/Cycle.php", "f", "$this->never_found");
    assert!(matches!(
        missing.outcome,
        CandidateOutcome::NoCandidate { .. }
    ));
}
