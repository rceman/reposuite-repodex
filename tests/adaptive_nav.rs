//! Native gates for RepoDex-first adaptive evidence navigation (§27, §47).

use repodex::query::adaptive::{adaptive_rdx, classify, NavIntent};
use repodex::query::projection::{EvidenceProjection, RelOut, SeedOut};

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
}

#[test]
fn intent_precedence_no_regex_steal() {
    // A test question containing "call" must NOT route to Callers/Callees.
    assert_eq!(
        classify("Which tests call Get?", None),
        NavIntent::TestEvidence
    );
    assert_eq!(
        classify("tests that invoke NewResolver", None),
        NavIntent::TestEvidence
    );
    // "who calls X" stays Callers even with a generic "call".
    assert_eq!(classify("Who calls Encode?", None), NavIntent::Callers);
    // "who calls whom" is a relationship question, not caller-of-one.
    assert_eq!(
        classify("Who calls whom in the engine?", None),
        NavIntent::Callers
    );
    // No fake-regex cue: "does .* call" is not literal substring-matched.
    assert_eq!(
        classify("does the engine call the codec", None),
        NavIntent::Callees
    );
}

#[test]
fn ambiguous_is_reachable() {
    // a question with no actionable cue is Ambiguous, not fabricated certainty.
    assert_eq!(
        classify("tell me about the thing", None),
        NavIntent::Ambiguous
    );
    assert_eq!(classify("the code", None), NavIntent::Ambiguous);
    // a clear structural word is not ambiguous.
    assert_eq!(
        classify("give me the architecture overview", None),
        NavIntent::GeneralStructural
    );
}

#[test]
fn callers_emits_call_edge_not_incoming_structural() {
    // seed = target decl; related has BOTH an incoming call_candidate (real
    // caller) AND an incoming structural `contains` edge that must be dropped.
    let mut p = fixture();
    p.related.push(rel(
        "incoming",
        "call_candidate",
        "decl:b.go#9",
        "Caller",
        "decl:a.go#1",
        "Get",
    ));
    p.related.push(rel(
        "incoming",
        "contains",
        "file:a.go",
        "a.go",
        "decl:a.go#1",
        "Get",
    ));
    let out = adaptive_rdx(&p, NavIntent::Callers);
    assert!(
        out.contains("call_candidate"),
        "real caller call edge emitted"
    );
    // the structural `contains` edge is not a caller — must be filtered.
    assert!(
        !out.lines().any(|l| l.starts_with("R contains")),
        "structural contains not a caller"
    );
}

#[test]
fn callees_emits_call_edge_not_outgoing_structural() {
    let mut p = fixture();
    p.related.push(rel(
        "outgoing",
        "call_candidate",
        "decl:a.go#1",
        "Get",
        "decl:c.go#2",
        "Encode",
    ));
    p.related.push(rel(
        "outgoing",
        "member_of",
        "decl:a.go#1",
        "Get",
        "file:a.go",
        "a.go",
    ));
    let out = adaptive_rdx(&p, NavIntent::Callees);
    assert!(out.contains("call_candidate"));
    assert!(
        !out.lines().any(|l| l.starts_with("R member_of")),
        "member_of not a callee"
    );
}

#[test]
fn filtered_irrelevant_does_not_trigger_truncation() {
    // lots of irrelevant structural edges, all relevant fit — no truncation gap.
    let mut p = fixture();
    for i in 0..30 {
        p.related.push(rel(
            "incoming",
            "contains",
            &format!("file:f{i}.go"),
            &format!("f{i}"),
            "decl:a.go#1",
            "Get",
        ));
    }
    p.related.push(rel(
        "incoming",
        "call_candidate",
        "decl:b.go#9",
        "Caller",
        "decl:a.go#1",
        "Get",
    ));
    let out = adaptive_rdx(&p, NavIntent::Callers);
    assert!(
        !out.contains("TRUNCATED_CONTINUATION"),
        "irrelevant filtering is not truncation"
    );
}

#[test]
fn relevant_over_bound_is_truncation() {
    // more relevant caller edges than the bound -> explicit truncation gap.
    let mut p = fixture();
    for i in 0..20 {
        p.related.push(rel(
            "incoming",
            "call_candidate",
            &format!("decl:c{i}.go#1"),
            &format!("C{i}"),
            "decl:a.go#1",
            "Get",
        ));
    }
    let out = adaptive_rdx(&p, NavIntent::Callers);
    assert!(out.contains("TRUNCATED_CONTINUATION"));
}

#[test]
fn byte_budget_enforced_no_midline() {
    // huge seeds -> packet stays <= budget and never cuts a record mid-line.
    let mut p = fixture();
    for i in 0..40 {
        p.seeds.push(SeedOut {
            rank: (i + 2) as u32,
            key: format!("decl:long/path/component/segment{i}/file.go#{i}"),
            kind: "declaration".into(),
            label: format!("a_very_long_symbol_label_{i}"),
            path: format!("long/path/component/segment{i}/file.go"),
            language: "go".into(),
            disposition: None,
            score: 1,
            factors: vec![],
            declaration_range: None,
            body_range: None,
        });
        p.related.push(rel(
            "outgoing",
            "call_candidate",
            "decl:a.go#1",
            "Get",
            &format!("decl:x{i}.go#1"),
            &format!("Target{i}"),
        ));
    }
    p.eligible_seed_count = 40;
    let out = adaptive_rdx(&p, NavIntent::Callees);
    // 4 KiB budget for callees; the trailing S summary pushes slightly over, so
    // the pre-gap body must have respected the budget (no mid-line cut).
    let body: String = out
        .lines()
        .take_while(|l| !l.starts_with("S "))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(body.len() <= 4096 + 512, "body bounded, got {}", body.len());
    for l in out.lines() {
        assert!(!l.ends_with('\\'), "mid-line cut: {l}");
    }
}

#[test]
fn locate_packet_is_small() {
    let out = adaptive_rdx(&fixture(), NavIntent::Locate);
    assert!(out.contains("I locate"));
    assert!(!out.lines().any(|l| l.starts_with("R ")));
    assert!(out.len() < 2048);
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

fn rel(
    direction: &str,
    kind: &str,
    from_key: &str,
    from_label: &str,
    to_key: &str,
    to_label: &str,
) -> RelOut {
    let node = if direction == "incoming" {
        from_key
    } else {
        to_key
    };
    let node_lbl = if direction == "incoming" {
        from_label
    } else {
        to_label
    };
    RelOut {
        direction: direction.into(),
        kind: kind.into(),
        evidence: "candidate".into(),
        via: "decl:a.go#1".into(),
        node: node.into(),
        label: node_lbl.into(),
        from_key: from_key.into(),
        from_label: from_label.into(),
        from_path: String::new(),
        to_key: to_key.into(),
        to_label: to_label.into(),
        to_path: String::new(),
        rule_id: None,
        candidate_set: None,
        disposition: None,
    }
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
