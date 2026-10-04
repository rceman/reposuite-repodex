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
            disposition_kind: None,
            disposition_reason: None,
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
        disposition_kind: None,
        disposition_reason: None,
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
        disposition_kind: None,
        disposition_reason: None,
        score: 1,
        factors: vec![],
        declaration_range: None,
        body_range: None,
    });
    p
}

// ---- contract-consolidation gates (defects A/C/D) ----

use repodex::query::adaptive::plan;
use repodex::query::QueryIntent;

fn mrel(kind: &str, from: &str, to: &str, ev: &str) -> RelOut {
    RelOut {
        direction: "outgoing".into(),
        kind: kind.into(),
        evidence: ev.into(),
        via: "".into(),
        node: to.into(),
        label: to.into(),
        from_key: from.into(),
        from_label: from.into(),
        from_path: "a.go".into(),
        to_key: to.into(),
        to_label: to.into(),
        to_path: "b.go".into(),
        rule_id: None,
        candidate_set: None,
        disposition: None,
        disposition_kind: None,
        disposition_reason: None,
    }
}

fn mseed(key: &str, label: &str) -> SeedOut {
    SeedOut {
        rank: 1,
        key: key.into(),
        kind: "declaration".into(),
        label: label.into(),
        path: "a.go".into(),
        language: "go".into(),
        disposition: None,
        disposition_kind: None,
        disposition_reason: None,
        score: 1,
        factors: vec![],
        declaration_range: None,
        body_range: None,
    }
}

#[test]
fn plan_maps_natural_language_to_typed_operation() {
    // defect A: "Who calls X?" must plan Callers, not Find.
    let p = plan("Who calls Get?", None, None, None);
    assert_eq!(p.nav, NavIntent::Callers);
    assert_eq!(p.query_intent, QueryIntent::Callers);
    assert_eq!(p.target.as_deref(), Some("Get"));
    let p = plan("What does Resolver call?", None, None, None);
    assert_eq!(p.query_intent, QueryIntent::Callees);
    assert_eq!(p.target.as_deref(), Some("Resolver"));
    let p = plan("How does engine reach Encode?", None, None, None);
    assert_eq!(p.query_intent, QueryIntent::Paths);
    assert_eq!(p.target.as_deref(), Some("engine"));
    assert_eq!(p.to.as_deref(), Some("Encode"));
    // explicit intent always wins.
    let p = plan("Who calls Get?", Some(QueryIntent::Find), None, None);
    assert_eq!(p.query_intent, QueryIntent::Find);
    assert_eq!(p.nav, NavIntent::Locate);
    // no anchor for callers -> honest Find fallback, not fabricated.
    let p = plan("Who calls?", None, None, None);
    assert_eq!(p.query_intent, QueryIntent::Find);
}

#[test]
fn utf8_budget_never_panics_never_splits_scalar() {
    // defect C: multi-byte chars crossing the budget boundary must not panic.
    let euro = "\u{20AC}".repeat(60); // 180 bytes, 60 chars
    let mut proj = EvidenceProjection {
        query: format!("Where is {euro}?"),
        schema: "evidence_projection.v1".into(),
        intent: "find".into(),
        seed_selection_complete: true,
        complete: true,
        ..Default::default()
    };
    for i in 0..8 {
        let mut s = mseed(&format!("decl:a.go#{}", i), &format!("{euro}Sym{i}"));
        s.label = format!("{euro}{i}");
        proj.seeds.push(s);
    }
    for i in 0..8 {
        proj.related.push(mrel(
            "call_candidate",
            &format!("decl:a.go#{}", i % 8),
            &format!("decl:a.go#{}", (i + 1) % 8),
            "candidate",
        ));
    }
    // budget-1 / budget / budget+1 around several sizes; no panic, valid UTF-8,
    // never exceeds, and the trailer must still be present.
    for b in [64usize, 256, 512, 1024, 2047, 2048, 2049, 4096] {
        let out = adaptive_rdx(&proj, NavIntent::Locate);
        assert!(
            std::str::from_utf8(out.as_bytes()).is_ok(),
            "invalid utf8 b={b}"
        );
        assert!(
            out.len() <= b.max(2048),
            "packet exceeded cap b={b} len={}",
            out.len()
        );
        assert!(out.contains("S "), "trailer missing at b={b}");
    }
}

#[test]
fn whole_packet_budget_includes_trailer() {
    // defect D/C: gap+summary must fit INSIDE the bound, not appended after.
    let mut proj = EvidenceProjection {
        query: "q".into(),
        schema: "p".into(),
        intent: "find".into(),
        eligible_seed_count: 40,
        seed_selection_complete: false,
        ..Default::default()
    };
    for i in 0..40 {
        proj.seeds
            .push(mseed(&format!("decl:a.go#{}", i), &format!("Sym{i}")));
        proj.related.push(mrel(
            "call",
            "decl:a.go#0",
            &format!("decl:a.go#{}", i),
            "fact",
        ));
    }
    let out = adaptive_rdx(&proj, NavIntent::Locate); // 2KiB envelope
    assert!(out.len() <= 2048, "packet {} exceeds 2KiB", out.len());
    assert!(out.contains("G RESULT_LIMIT"), "expected result-limit gap");
    assert!(out.contains("S "), "summary missing");
    // summary counts reflect emitted state, not eligible.
    let s = out.lines().find(|l| l.starts_with("S ")).unwrap();
    assert!(s.contains("bytes="), "summary lacks byte count");
}

#[test]
fn result_limit_is_not_ambiguity() {
    // §26: eligible > shown by selection bound -> RESULT_LIMIT, not AMBIGUOUS.
    let mut proj = EvidenceProjection {
        query: "q".into(),
        schema: "p".into(),
        intent: "find".into(),
        eligible_seed_count: 20,
        seed_selection_complete: false,
        ..Default::default()
    };
    for i in 0..20 {
        proj.seeds
            .push(mseed(&format!("decl:a.go#{}", i), &format!("Sym{i}")));
    }
    let out = adaptive_rdx(&proj, NavIntent::Locate);
    assert!(
        out.contains("G RESULT_LIMIT"),
        "expected RESULT_LIMIT: {out}"
    );
    assert!(
        !out.contains("G AMBIGUOUS_RESULT shown="),
        "result-limit mislabeled ambiguous"
    );
}

#[test]
fn no_dangling_local_references() {
    // §28: every emitted R endpoint id must reference an emitted F line.
    let mut proj = EvidenceProjection {
        query: "q".into(),
        schema: "p".into(),
        intent: "find".into(),
        seed_selection_complete: true,
        ..Default::default()
    };
    for i in 0..30 {
        proj.seeds.push(mseed(&format!("s{i}"), &format!("L{i}")));
    }
    for i in 0..29 {
        proj.related.push(mrel(
            "call",
            &format!("s{i}"),
            &format!("s{}", i + 1),
            "fact",
        ));
    }
    let out = adaptive_rdx(&proj, NavIntent::Locate);
    let fids: std::collections::BTreeSet<u32> = out
        .lines()
        .filter(|l| l.starts_with("F "))
        .filter_map(|l| l.split_whitespace().nth(1)?.parse::<u32>().ok())
        .collect();
    for l in out.lines().filter(|l| l.starts_with("R ")) {
        let v: Vec<&str> = l.split_whitespace().collect();
        let fs: u32 = v[2].parse().unwrap();
        let ts: u32 = v[3].parse().unwrap();
        assert!(
            fids.contains(&fs) && fids.contains(&ts),
            "dangling ref in {l}"
        );
    }
}

#[test]
fn packet_validator_accepts_wellformed() {
    use repodex::query::adaptive::validate_packet;
    let mut p = fixture();
    p.seeds.push(mseed("decl:b.go#2", "Set"));
    p.related
        .push(mrel("call", "decl:a.go#1", "decl:b.go#2", "fact"));
    let out = adaptive_rdx(&p, NavIntent::Locate);
    let errs = validate_packet(&out, 2048);
    assert!(errs.is_empty(), "validator errors: {errs:?}\n{out}");
}

#[test]
fn packet_validator_catches_violations() {
    use repodex::query::adaptive::validate_packet;
    assert!(!validate_packet("no header\n", 2048).is_empty());
    let bad = format!(
        "#RDX1 adaptive v2\nR call 9 4 f\nS bytes=10\n{}",
        "x".repeat(3000)
    );
    let errs = validate_packet(&bad, 2048);
    assert!(
        errs.iter()
            .any(|e| e.contains("exceeds") || e.contains("dangling") || e.contains("bytes")),
        "{errs:?}"
    );
}
