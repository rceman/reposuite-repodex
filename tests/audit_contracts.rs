//! PRE-SYSTEM-ONE external contract audit — adversarial tests around the
//! deterministic query/RDX1 contracts. These do NOT rely on the
//! implementation's own happy-path tests; they probe the public surface for
//! §3–§28 contract violations.
//!
//! Fixture: a mixed Rust+Go repo. `dup` has `helper()` in three files -> a
//! MultipleCandidates set; `run.go` has an imported `auth.ValidateToken()`,
//! an external `x.Do()` (OutOfScope) and a local `localCall()` (NoCandidate is
//! produced for an unresolved local).

mod support;

use repodex::candidates::build_candidates;
use repodex::graph::{build_graph, EvidenceClass, GraphIndex, LookupDomain, NodeKind, PathOptions};
use repodex::links::build_links;
use repodex::query::{QueryEngine, QueryIntent, QueryMode, QueryPlan};
use repodex::rdx1;
use repodex::repository::{build_snapshot, BuildOptions};
use support::TempDir;

fn graph_of(files: &[(&str, &str)]) -> (TempDir, GraphIndex) {
    let t = TempDir::new("audit");
    let repo = t.path().join("repo");
    for (rel, src) in files {
        let p = repo.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, src).unwrap();
    }
    let snap = t.path().join("snap");
    build_snapshot(&support::analyzer(), &repo, &snap, BuildOptions::default()).expect("snap");
    let links = t.path().join("links");
    build_links(&snap, Some(&repo), &links).expect("links");
    let cand = t.path().join("cand");
    build_candidates(&snap, &links, &cand).expect("cand");
    let g = t.path().join("graph");
    build_graph(&snap, &links, &cand, &g).expect("graph");
    (t, GraphIndex::load(&g).expect("load"))
}

fn fixture() -> Vec<(&'static str, &'static str)> {
    vec![
        ("go.mod", "module example.com/m\n\ngo 1.22\n"),
        (
            "auth/token.go",
            "package auth\nfunc ValidateToken() {}\nfunc IssueToken() {}\n",
        ),
        (
            "app/run.go",
            "package app\nimport \"example.com/m/auth\"\nimport \"example.com/ext/x\"\nfunc run() {\n\tauth.ValidateToken()\n\tx.Do()\n\tlocalCall()\n}\nfunc localCall() {}\n",
        ),
        // three same-name package funcs -> MultipleCandidates set of 3
        ("dup/a.go", "package dup\nfunc helper() {}\n"),
        ("dup/b.go", "package dup\nfunc helper() {}\n"),
        ("dup/c.go", "package dup\nfunc helper() {}\nfunc run() { helper() }\n"),
    ]
}

fn engine<'a>(g: &'a GraphIndex) -> QueryEngine<'a> {
    QueryEngine::new(g)
}

fn query(
    g: &GraphIndex,
    raw: &str,
    mode: QueryMode,
    intent: Option<QueryIntent>,
    target: Option<String>,
    max: usize,
    tokens: Option<usize>,
) -> repodex::query::QueryResult {
    let e = engine(g);
    let p = QueryPlan::parse(raw, mode, intent, target, None, max, tokens);
    p.validate().expect("plan valid");
    e.run(&p)
}

// ---------------------------------------------------------------------------
// §3 — exhaustive + output-budget: enumeration identical, only presentation changes
// ---------------------------------------------------------------------------

#[test]
fn exhaustive_budget_matrix_identical_enumeration() {
    let (_t, g) = graph_of(&fixture());
    let budgets = [None, Some(1_000_000), Some(60), Some(30)];
    let mut enumeration = None;
    for b in budgets {
        let r = query(
            &g,
            "token",
            QueryMode::Exhaustive,
            None,
            None,
            usize::MAX,
            b,
        );
        // total (the enumeration) must be identical across budgets
        if let Some(t) = enumeration {
            assert_eq!(t, r.total, "enumeration changed by output budget");
        } else {
            enumeration = Some(r.total);
        }
        if r.total > r.shown {
            assert!(!r.complete, "complete=true after truncation");
            assert_eq!(
                r.truncated_reason.as_deref(),
                Some("budget"),
                "wrong reason"
            );
        }
    }
}

#[test]
fn exhaustive_budget_consistent_in_json_and_rdx1() {
    let (_t, g) = graph_of(&fixture());
    let r = query(
        &g,
        "token",
        QueryMode::Exhaustive,
        None,
        None,
        usize::MAX,
        Some(30),
    );
    assert!(!r.complete);
    let doc = rdx1::parse(&rdx1::render(&r)).unwrap();
    assert_eq!(doc.summary.get("complete").map(String::as_str), Some("0"));
    assert_eq!(doc.summary.get("truncated").map(String::as_str), Some("1"));
    assert_eq!(
        doc.summary.get("reason").map(String::as_str),
        Some("budget")
    );
    assert_eq!(
        doc.summary
            .get("total")
            .map(String::as_str)
            .unwrap()
            .parse::<usize>()
            .unwrap(),
        r.total
    );
}

// ---------------------------------------------------------------------------
// §4 — zero-result vs truncated-to-zero must be observationally distinct
// ---------------------------------------------------------------------------

#[test]
fn zero_result_vs_truncated_zero_distinct() {
    let (_t, g) = graph_of(&fixture());
    let zero = query(
        &g,
        "zzz_no_such",
        QueryMode::Exhaustive,
        None,
        None,
        usize::MAX,
        None,
    );
    let trunc = query(
        &g,
        "token",
        QueryMode::Exhaustive,
        None,
        None,
        usize::MAX,
        Some(1),
    );
    let zd = rdx1::parse(&rdx1::render(&zero)).unwrap();
    let td = rdx1::parse(&rdx1::render(&trunc)).unwrap();
    // zero: shown=0 total=0 complete=1, no truncation marker
    assert_eq!(zd.summary["total"], "0");
    assert_eq!(zd.summary["complete"], "1");
    assert!(!zd.summary.contains_key("truncated"));
    // truncated-to-zero: shown=0 total>0 complete=0 truncated=1
    assert_eq!(td.summary["shown"], "0");
    assert!(td.summary["total"].parse::<usize>().unwrap() > 0);
    assert_eq!(td.summary["complete"], "0");
    assert_eq!(td.summary["truncated"], "1");
    assert_ne!(zd.summary["total"], td.summary["total"]); // not equivalent
}

// ---------------------------------------------------------------------------
// §5 — ranked prefix invariance
// ---------------------------------------------------------------------------

#[test]
fn ranked_prefix_invariance() {
    let (_t, g) = graph_of(&fixture());
    let keys = |max: usize, tok: Option<usize>| -> Vec<String> {
        query(&g, "token", QueryMode::Ranked, None, None, max, tok)
            .seeds
            .iter()
            .map(|s| s.node.key.clone())
            .collect()
    };
    let all = keys(usize::MAX, None);
    for n in [1usize, 3, 5] {
        let prefix = keys(n, None);
        assert_eq!(
            prefix,
            all[..n.min(all.len())].to_vec(),
            "RANKING_PREFIX_ERROR at n={n}"
        );
    }
    // token budget must not reorder the retained prefix
    let budgeted = keys(usize::MAX, Some(40));
    assert_eq!(
        budgeted,
        all[..budgeted.len()].to_vec(),
        "budget reordered ranking"
    );
}

// ---------------------------------------------------------------------------
// §6 — tie ordering is deterministic and canonical
// ---------------------------------------------------------------------------

#[test]
fn tie_order_is_canonical() {
    let (_t, g) = graph_of(&fixture());
    // the three `helper` decls all match term `helper` with equal factors;
    // ties must break by canonical key, repeatably.
    let a: Vec<_> = query(
        &g,
        "helper",
        QueryMode::Ranked,
        None,
        None,
        usize::MAX,
        None,
    )
    .seeds
    .iter()
    .map(|s| (s.node.key.clone(), s.score))
    .collect();
    let b: Vec<_> = query(
        &g,
        "helper",
        QueryMode::Ranked,
        None,
        None,
        usize::MAX,
        None,
    )
    .seeds
    .iter()
    .map(|s| (s.node.key.clone(), s.score))
    .collect();
    assert_eq!(a, b);
    // equal scores are sorted by key
    let helper: Vec<_> = a
        .iter()
        .filter(|(_, s)| *s == a[0].1)
        .map(|(k, _)| k.clone())
        .collect();
    let mut sorted = helper.clone();
    sorted.sort();
    assert_eq!(helper, sorted);
}

// ---------------------------------------------------------------------------
// §7 — a path containing a CANDIDATE edge is never a definite call path
// ---------------------------------------------------------------------------

#[test]
fn candidate_path_never_labeled_definite() {
    let (_t, g) = graph_of(&fixture());
    let file = g.node("file:app/run.go").unwrap();
    let validate = g.find("ValidateToken", LookupDomain::Name)[0];
    let paths = g.paths(&file.node_id, &validate.node_id, &PathOptions::default());
    assert!(!paths.is_empty());
    for p in &paths {
        if p.has_candidate {
            assert!(
                p.evidence_label().contains("candidate"),
                "CANDIDATE_PATH_PROMOTED_TO_FACT: {}",
                p.evidence_label()
            );
            assert!(!p.evidence_label().contains("definite"));
            assert!(!p.evidence_label().contains("exact"));
        }
    }
}

// ---------------------------------------------------------------------------
// §8 — callers/callees wording preserves candidate (not exact) semantics
// ---------------------------------------------------------------------------

#[test]
fn callers_report_candidate_not_exact() {
    let (_t, g) = graph_of(&fixture());
    let r = query(
        &g,
        "ValidateToken",
        QueryMode::Ranked,
        Some(QueryIntent::Callers),
        Some("ValidateToken".into()),
        50,
        None,
    );
    for h in &r.related {
        assert_eq!(h.evidence, EvidenceClass::Candidate);
        assert!(h.candidate_set.is_some());
    }
}

// ---------------------------------------------------------------------------
// §9/§10 — MultipleCandidates preserved end-to-end, atomic under budget
// ---------------------------------------------------------------------------

#[test]
fn multiple_candidates_preserved_and_atomic() {
    let (_t, g) = graph_of(&fixture());
    // the `helper()` call in dup/c.go has a 3-way MultipleCandidates set.
    let call = g
        .nodes()
        .iter()
        .find(|n| n.kind == NodeKind::Call && n.path == "dup/c.go" && n.label == "helper")
        .expect("helper call");
    assert_eq!(call.disposition.as_deref(), Some("multiple_candidates"));
    let out = g.outgoing(&call.node_id);
    let cands: Vec<_> = out.iter().filter(|e| e.kind == "call_candidate").collect();
    assert_eq!(cands.len(), 3, "expected 3 candidate targets");
    // all share one candidate_set_id — never collapsed to SingleCandidate
    let sets: std::collections::BTreeSet<_> = cands
        .iter()
        .map(|e| e.candidate_set_id.clone().unwrap())
        .collect();
    assert_eq!(
        sets.len(),
        1,
        "MULTIPLE_COLLAPSED_TO_SINGLE / CANDIDATE_SET_ID_LOST"
    );
    // survive into RDX1: query callers of any `helper` decl -> set id present
    let doc = rdx1::parse(&rdx1::render(&query(
        &g,
        "helper",
        QueryMode::Ranked,
        Some(QueryIntent::Callers),
        Some("helper".into()),
        50,
        None,
    )))
    .unwrap();
    let csets: std::collections::BTreeSet<_> = doc
        .relations
        .iter()
        .filter_map(|r| r.candidate_set.clone())
        .collect();
    assert!(
        csets.iter().all(|s| s.starts_with("cand-")),
        "candidate sets preserved"
    );
}

// ---------------------------------------------------------------------------
// §11 — NoCandidate / OutOfScope survive to summary
// ---------------------------------------------------------------------------

#[test]
fn no_candidate_and_out_of_scope_propagate() {
    let (_t, g) = graph_of(&fixture());
    // x.Do is external -> OutOfScope; it must exist as a call node with a disposition
    let ext = g
        .nodes()
        .iter()
        .find(|n| n.kind == NodeKind::Call && n.label == "x.Do")
        .expect("x.Do");
    assert_eq!(
        ext.disposition.as_deref(),
        Some("out_of_scope:external_import")
    );
    // it emits no candidate edge but is still a graph node (not silently dropped)
    assert!(g
        .outgoing(&ext.node_id)
        .iter()
        .all(|e| e.kind != "call_candidate"));
}

// ---------------------------------------------------------------------------
// §12 — RDX1 summary truth table
// ---------------------------------------------------------------------------

#[test]
fn rdx1_summary_truth_table() {
    let (_t, g) = graph_of(&fixture());
    // A: complete non-empty
    let d = rdx1::parse(&rdx1::render(&query(
        &g,
        "ValidateToken",
        QueryMode::Ranked,
        None,
        None,
        50,
        None,
    )))
    .unwrap();
    assert_eq!(d.summary["complete"], "1");
    assert!(d.summary["shown"].parse::<u32>().unwrap() > 0);
    // B: complete zero
    let d = rdx1::parse(&rdx1::render(&query(
        &g,
        "zzz",
        QueryMode::Ranked,
        None,
        None,
        50,
        None,
    )))
    .unwrap();
    assert_eq!(d.summary["complete"], "1");
    assert_eq!(d.summary["total"], "0");
    // C: ranked limited
    let d = rdx1::parse(&rdx1::render(&query(
        &g,
        "token",
        QueryMode::Ranked,
        None,
        None,
        2,
        None,
    )))
    .unwrap();
    assert_eq!(d.summary["shown"], "2");
    // D: exhaustive budget-truncated
    let d = rdx1::parse(&rdx1::render(&query(
        &g,
        "token",
        QueryMode::Exhaustive,
        None,
        None,
        usize::MAX,
        Some(20),
    )))
    .unwrap();
    assert_eq!(d.summary["complete"], "0");
    assert_eq!(d.summary["reason"], "budget");
    // F: MultipleCandidates — every cand relation keeps a cs
    let d = rdx1::parse(&rdx1::render(&query(
        &g,
        "helper",
        QueryMode::Ranked,
        Some(QueryIntent::Callers),
        Some("helper".into()),
        50,
        None,
    )))
    .unwrap();
    assert!(d
        .relations
        .iter()
        .filter(|r| r.evidence == 'c')
        .all(|r| r.candidate_set.is_some()));
}

// ---------------------------------------------------------------------------
// §13 — RDX1 unknown-field / version robustness
// ---------------------------------------------------------------------------

#[test]
fn rdx1_unknown_fields_tolerated_and_bad_version_rejected() {
    // unknown S keys and unknown records are ignored (forward-compatible)
    let doc =
        rdx1::parse("#RDX1 v1\nQ x\nF 1 decl k L\nS shown=1 novel_field=9\nX unknown-record\n")
            .unwrap();
    assert_eq!(doc.summary["novel_field"], "9");
    assert_eq!(doc.facts.len(), 1);
    // missing header is rejected
    assert!(rdx1::parse("F 1 decl k L\n").is_err());
}

// ---------------------------------------------------------------------------
// §14 — escaping adversarial cases round-trip
// ---------------------------------------------------------------------------

#[test]
fn rdx1_escaping_round_trip() {
    // a label with spaces/quotes/backslash/unicode must survive emit->parse
    let weird = "wei\"rd \\ name\tλ函数";
    let esc = |s: &str| -> String {
        let mut o = String::from("\"");
        for c in s.chars() {
            match c {
                '"' => o.push_str("\\\""),
                '\\' => o.push_str("\\\\"),
                _ => o.push(c),
            }
        }
        o.push('"');
        o
    };
    let line = format!("#RDX1 v1\nF 1 decl key {}", esc(weird));
    let doc = rdx1::parse(&line).unwrap();
    assert_eq!(doc.facts[0].label, weird);
}

// ---------------------------------------------------------------------------
// §18/§19 — QueryPlan determinism + validation
// ---------------------------------------------------------------------------

#[test]
fn query_plan_deterministic_and_validated() {
    let a = QueryPlan::parse("auth token", QueryMode::Ranked, None, None, None, 50, None);
    let b = QueryPlan::parse("auth  token", QueryMode::Ranked, None, None, None, 50, None);
    assert_eq!(a.terms, b.terms); // whitespace-normalized
    let c = QueryPlan::parse("AuthToken", QueryMode::Ranked, None, None, None, 50, None);
    assert_eq!(c.terms, vec!["auth", "token"]); // camel -> same terms
                                                // validation
    assert!(
        QueryPlan::parse("x", QueryMode::Ranked, None, None, None, 0, None)
            .validate()
            .is_err()
    );
    assert!(
        QueryPlan::parse("x", QueryMode::Ranked, None, None, None, 50, Some(0))
            .validate()
            .is_err()
    );
    assert!(QueryPlan::parse(
        "x",
        QueryMode::Ranked,
        Some(QueryIntent::Callers),
        None,
        None,
        50,
        None
    )
    .validate()
    .is_err());
    assert!(QueryPlan::parse(
        "x",
        QueryMode::Ranked,
        Some(QueryIntent::Paths),
        Some("a".into()),
        None,
        50,
        None
    )
    .validate()
    .is_err());
}

// ---------------------------------------------------------------------------
// §21/§22 — every semantic field is integrity-covered; tampering is rejected
// ---------------------------------------------------------------------------

/// Mutate every row of one artifact file; return true if load then FAILS.
fn tamper_rejected(
    g_dir: &std::path::Path,
    mutate: impl Fn(&mut serde_json::Value),
    file: &str,
    tag: &str,
) -> bool {
    let dir = g_dir.parent().unwrap().join(format!("tamper-{tag}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for name in ["manifest.json", "nodes.jsonl", "edges.jsonl"] {
        std::fs::copy(g_dir.join(name), dir.join(name)).unwrap();
    }
    let path = dir.join(file);
    let mut rows: Vec<serde_json::Value> = std::fs::read_to_string(&path)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    for r in rows.iter_mut() {
        mutate(r);
    }
    std::fs::write(
        &path,
        rows.iter()
            .map(|r| r.to_string())
            .collect::<Vec<_>>()
            .join("\n")
            + "\n",
    )
    .unwrap();
    GraphIndex::load(&dir).is_err()
}

#[test]
fn semantic_field_tampering_is_rejected() {
    let (t, _g) = graph_of(&fixture());
    let gdir = t.path().join("graph");
    assert!(
        tamper_rejected(
            &gdir,
            |r| {
                if r["kind"] == "declaration" {
                    r["label"] = "X".into()
                }
            },
            "nodes.jsonl",
            "label"
        ),
        "UNDIGESTED node label"
    );
    assert!(
        tamper_rejected(
            &gdir,
            |r| {
                if r["kind"] == "call_candidate" {
                    r["rule_id"] = "evil.rule".into()
                }
            },
            "edges.jsonl",
            "rule"
        ),
        "UNDIGESTED rule_id"
    );
    assert!(
        tamper_rejected(
            &gdir,
            |r| {
                if r["kind"] == "contains" {
                    r["evidence_class"] = "candidate".into()
                }
            },
            "edges.jsonl",
            "ev"
        ),
        "UNDIGESTED evidence class"
    );
    assert!(
        tamper_rejected(
            &gdir,
            |r| {
                if r["kind"] == "call_candidate" {
                    r["candidate_set_id"] = "cand-forged".into()
                }
            },
            "edges.jsonl",
            "cs"
        ),
        "UNDIGESTED candidate_set_id"
    );
    assert!(
        tamper_rejected(
            &gdir,
            |r| {
                if r["kind"] == "call_candidate" {
                    r["target"] = "gn-0000000000000000".into()
                }
            },
            "edges.jsonl",
            "tgt"
        ),
        "UNDIGESTED edge target"
    );
    assert!(
        tamper_rejected(
            &gdir,
            |r| {
                if r["kind"] == "call" {
                    r["disposition"] = "single_candidate".into()
                }
            },
            "nodes.jsonl",
            "disp"
        ),
        "UNDIGESTED disposition"
    );
}
