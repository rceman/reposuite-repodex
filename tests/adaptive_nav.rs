//! Native gates for RepoDex-first adaptive evidence navigation (§27).

use repodex::query::adaptive::{adaptive_rdx, classify, NavIntent};
use repodex::query::projection::{EvidenceProjection, SeedOut};

#[test]
fn classify_is_deterministic() {
    assert_eq!(classify("Where is Get?", None), NavIntent::Locate);
    assert_eq!(classify("Who calls ResolveKey?", None), NavIntent::Callers);
    assert_eq!(
        classify("What functions does main call?", None),
        NavIntent::Callees
    );
    assert_eq!(
        classify("How does engine reach util Encode?", None),
        NavIntent::PathOrFlow
    );
    assert_eq!(
        classify("What does go.mod declare?", None),
        NavIntent::ManifestConfig
    );
    assert_eq!(
        classify("Which tests cover the cache?", None),
        NavIntent::TestEvidence
    );
    assert_eq!(
        classify("Who calls X?", None),
        classify("Who calls X?", None)
    );
}

#[test]
fn locate_packet_is_small() {
    let p = fixture();
    let out = adaptive_rdx(&p, NavIntent::Locate);
    assert!(out.contains("I locate"));
    assert!(!out.lines().any(|l| l.starts_with("R ")));
    assert!(out.len() < 2048);
}

#[test]
fn callers_packet_has_caller_edge() {
    let p = fixture();
    assert!(adaptive_rdx(&p, NavIntent::Callers).contains("I callers"));
}

#[test]
fn no_duplicate_entity_lines() {
    let out: String = adaptive_rdx(&fixture(), NavIntent::GeneralStructural);
    let mut ids = std::collections::HashSet::<String>::new();
    for l in out.lines() {
        if let Some(rest) = l.strip_prefix("F ") {
            let id = rest.split_whitespace().next().unwrap();
            assert!(ids.insert(id.to_string()), "duplicate F id {id}");
        }
    }
}

#[test]
fn facts_candidates_preserved() {
    assert!(adaptive_rdx(&fixture(), NavIntent::GeneralStructural).contains(" candidates="));
}

fn fixture() -> EvidenceProjection {
    let mut p = EvidenceProjection {
        query: "q".into(),
        emitted_seed_count: 1,
        eligible_seed_count: 1,
        seed_selection_complete: true,
        ..Default::default()
    };
    p.seeds.push(SeedOut {
        rank: 1,
        key: "decl:a.go#1".into(),
        kind: "declaration".into(),
        label: "Get".into(),
        path: "a.go".into(),
        language: "go".into(),
        disposition: None,
        score: 1,
        factors: vec![],
        declaration_range: None,
        body_range: None,
    });
    p
}
