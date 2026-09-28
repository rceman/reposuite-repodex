//! Adaptive evidence navigation (RepoDex-first, V1).
//!
//! Deterministic intent classification + obligation-aware evidence-unit
//! selection + semantic dedup + budget, rendered as compact adaptive RDX.
//! Intent is routing metadata only — it never upgrades FACT/CANDIDATE (§9,§19).
//!
//! Pipeline: query text -> `NavIntent` -> evidence obligations -> bounded
//! selection over the canonical `EvidenceProjection` -> dedup -> budget ->
//! compact RDX. No LLM; no natural-language answer generation.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

use super::projection::{EvidenceProjection, RelOut, SeedOut};

/// Repository-navigation intents (§8). Routing metadata only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavIntent {
    /// "where is X" / "find X" — a locator packet.
    Locate,
    /// "what is X's declaration/signature".
    Definition,
    /// "who calls X" — bounded caller edges.
    Callers,
    /// "what does X call" — bounded callee edges.
    Callees,
    /// "how does A relate to B" — direction between anchors.
    Relationship,
    /// "how does A reach B" — bounded path/flow.
    PathOrFlow,
    /// "what manifest/config controls X".
    ManifestConfig,
    /// "which tests cover X".
    TestEvidence,
    /// broad structural orientation.
    GeneralStructural,
    /// cannot be safely classified — bounded evidence + explicit gap.
    Ambiguous,
}

impl NavIntent {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Locate => "locate",
            Self::Definition => "definition",
            Self::Callers => "callers",
            Self::Callees => "callees",
            Self::Relationship => "relationship",
            Self::PathOrFlow => "path_or_flow",
            Self::ManifestConfig => "manifest_or_config",
            Self::TestEvidence => "test_evidence",
            Self::GeneralStructural => "general_structural",
            Self::Ambiguous => "ambiguous",
        }
    }
}

/// Deterministic cue-based classification. No model. Order matters — more
/// specific cues win. `to`/relationship cues beat plain lookup.
pub fn classify(text: &str, plan_intent: Option<crate::query::QueryIntent>) -> NavIntent {
    use crate::query::QueryIntent as Q;
    let t = text.to_lowercase();
    // Explicit typed intent (internal callers/recipes) wins over text cues.
    if let Some(q) = plan_intent {
        return match q {
            Q::Callers => NavIntent::Callers,
            Q::Callees => NavIntent::Callees,
            Q::Paths => NavIntent::PathOrFlow,
            Q::Related => NavIntent::Relationship,
            Q::Find => NavIntent::Locate,
        };
    }
    let has = |w: &[&str]| w.iter().any(|s| t.contains(s));
    // path/flow & relationship first (need two anchors).
    if has(&[
        "reach",
        "path to",
        "path from",
        "route",
        "flow",
        "how does",
        "trace the call",
    ]) {
        return NavIntent::PathOrFlow;
    }
    // manifest/config before callees: "what does go.mod declare" is manifest,
    // not a call question.
    if has(&[
        "manifest",
        "cargo.toml",
        "go.mod",
        "config",
        "dependency",
        "does go.mod",
        "module does",
    ]) {
        return NavIntent::ManifestConfig;
    }
    if has(&[
        "who calls",
        "callers of",
        "called by",
        "who invokes",
        "call sites of",
    ]) {
        return NavIntent::Callers;
    }
    // Callees needs an explicit call-directed phrase, not just "what does".
    if has(&[
        "functions does",
        "does .* call",
        "calls to",
        "what does .* call",
        "callees",
        "call",
        "invoke",
        "uses",
    ]) {
        return NavIntent::Callees;
    }
    if has(&["test", "covers", "cover"]) {
        return NavIntent::TestEvidence;
    }
    if has(&["relat", "between", "depend", "who calls whom", "connect"]) {
        return NavIntent::Relationship;
    }
    if has(&[
        "signature",
        "declaration",
        "what is",
        "declares",
        "type of",
        "definition",
    ]) {
        return NavIntent::Definition;
    }
    if has(&["where is", "find", "locate", "which file", "what file"]) {
        return NavIntent::Locate;
    }
    // Nothing confident -> general structural (bounded), not fabricated intent.
    NavIntent::GeneralStructural
}

/// Evidence-obligation set selected for an intent (§10). Drives which related
/// edges + how many seeds to keep.
struct Obligation {
    /// Keep seeds up to this count.
    max_seeds: usize,
    /// Keep related edges up to this count (after relevance+dedup).
    max_related: usize,
    /// Whether to keep relationship edges at all.
    want_related: bool,
    /// Whether to keep connector/path output.
    want_path: bool,
}

fn obligation(intent: NavIntent) -> Obligation {
    match intent {
        NavIntent::Locate | NavIntent::Definition => Obligation {
            max_seeds: 3,
            max_related: 0,
            want_related: false,
            want_path: false,
        },
        NavIntent::Callers | NavIntent::Callees | NavIntent::Relationship => Obligation {
            max_seeds: 3,
            max_related: 16,
            want_related: true,
            want_path: false,
        },
        NavIntent::PathOrFlow => Obligation {
            max_seeds: 2,
            max_related: 8,
            want_related: true,
            want_path: true,
        },
        NavIntent::ManifestConfig => Obligation {
            max_seeds: 4,
            max_related: 8,
            want_related: true,
            want_path: false,
        },
        NavIntent::TestEvidence => Obligation {
            max_seeds: 4,
            max_related: 12,
            want_related: true,
            want_path: false,
        },
        NavIntent::GeneralStructural | NavIntent::Ambiguous => Obligation {
            max_seeds: 5,
            max_related: 10,
            want_related: true,
            want_path: false,
        },
    }
}

/// Is this edge relevant to the intent's evidence obligation? The edge `kind`
/// is a deterministic relation kind; `evidence` stays untouched (never
/// upgraded — candidates remain candidates).
fn edge_relevant(r: &RelOut, intent: NavIntent) -> bool {
    let incoming = r.direction == "incoming";
    match intent {
        NavIntent::Locate | NavIntent::Definition => false, // locator packet: no rel edges
        NavIntent::Callers => incoming,
        NavIntent::Callees => !incoming,
        NavIntent::Relationship => true, // both directions between anchors
        NavIntent::PathOrFlow => true,
        NavIntent::ManifestConfig => {
            // ownership/membership/manifest edges, not generic call edges
            matches!(
                r.kind.as_str(),
                "contains" | "owns" | "member_of" | "declares" | "manifest"
            )
        }
        NavIntent::TestEvidence => true,
        NavIntent::GeneralStructural | NavIntent::Ambiguous => true,
    }
}

/// Dedup key for a relationship: kind + direction-resolved endpoints + rule +
/// candidate set. Distinct witnesses (different rule/candidate set) are kept;
/// pure repeats collapse.
fn rel_key(r: &RelOut) -> String {
    format!(
        "{}|{}|{}|{}|{}",
        r.kind,
        r.from_key,
        r.to_key,
        r.rule_id.as_deref().unwrap_or(""),
        r.candidate_set.as_deref().unwrap_or("")
    )
}

/// Render the adaptive agent-facing RDX for a projection conditioned on the
/// classified intent. Locator intents emit a small packet; relationship
/// intents keep only obligation-relevant, deduped edges. FACT/CANDIDATE and
/// explicit gaps are preserved exactly.
pub fn adaptive_rdx(proj: &EvidenceProjection, intent: NavIntent) -> String {
    let ob = obligation(intent);
    let seeds: Vec<&SeedOut> = proj.seeds.iter().take(ob.max_seeds).collect();
    // Relevance-filter + dedup related edges.
    let mut seen = BTreeSet::new();
    let related: Vec<&RelOut> = if ob.want_related {
        proj.related
            .iter()
            .filter(|r| edge_relevant(r, intent))
            .filter(|r| seen.insert(rel_key(r)))
            .take(ob.max_related)
            .collect()
    } else {
        Vec::new()
    };

    // Local ids over the *selected* seed+neighbor keys only.
    let mut lid: BTreeMap<String, u32> = BTreeMap::new();
    let mut next = 1u32;
    let mut out = String::from("#RDX1 adaptive v2\n");
    let _ = writeln!(out, "Q {}", esc(&proj.query));
    let _ = writeln!(out, "I {}", intent.as_str());
    let mut seed_ids = Vec::new();
    for s in &seeds {
        let id = *lid.entry(s.key.clone()).or_insert_with(|| {
            let n = next;
            next += 1;
            n
        });
        // One canonical line per Agent-relevant node identity: seed kind wins;
        // a duplicate "node" alias for the same key is not emitted (§14).
        let _ = writeln!(out, "F {} {} {} {}", id, s.kind, esc(&s.key), esc(&s.label));
        seed_ids.push((id, s));
    }
    for r in &related {
        // neighbor endpoints only; a seed key is already emitted above.
        for k in [&r.from_key, &r.to_key] {
            if !lid.contains_key(k.as_str()) {
                let id = next;
                next += 1;
                lid.insert((*k).clone(), id);
                let _ = writeln!(
                    out,
                    "F {} node {} {}",
                    id,
                    esc(k),
                    esc(if *k == r.from_key {
                        &r.from_label
                    } else {
                        &r.to_label
                    })
                );
            }
        }
    }
    for (id, s) in &seed_ids {
        let mut d = format!("D {} rank={} path={}", id, s.rank, esc(&s.path));
        if let Some(r) = &s.declaration_range {
            let _ = write!(d, " decl={}:{}-{}", r.start_line, r.byte_start, r.byte_end);
        }
        if let Some(r) = &s.body_range {
            let _ = write!(d, " body={}:{}-{}", r.start_line, r.byte_start, r.byte_end);
        }
        out.push_str(&d);
        out.push('\n');
    }
    for r in &related {
        let fs = lid.get(&r.from_key).copied().unwrap_or(0);
        let ts = lid.get(&r.to_key).copied().unwrap_or(0);
        let ev = if r.evidence == "fact" { 'f' } else { 'c' };
        let mut line = format!("R {} {} {} {}", r.kind, fs, ts, ev);
        if let Some(cs) = &r.candidate_set {
            let _ = write!(line, " cs={cs}");
        }
        if let Some(rule) = &r.rule_id {
            let _ = write!(line, " rule={}", esc(rule));
        }
        out.push_str(&line);
        out.push('\n');
    }
    if ob.want_path {
        if let Some(c) = &proj.connector {
            for (i, rt) in c.routes.iter().enumerate() {
                let mut p = format!("P {} {}", i + 1, rt.evidence_label.replace(' ', "_"));
                for st in &rt.steps {
                    let _ = write!(
                        p,
                        " | {} {} {}",
                        esc(&st.from_key),
                        st.relation,
                        esc(&st.to_key)
                    );
                }
                out.push_str(&p);
                out.push('\n');
            }
            if !c.found {
                let _ = writeln!(out, "P none bounded_no_route depth<=3");
            }
        }
    }
    // Explicit gap lines (§20) — never silent omission.
    if proj.seeds.is_empty() {
        let _ = writeln!(out, "G NO_RESULT");
    } else if proj.eligible_seed_count > seeds.len() {
        let _ = writeln!(
            out,
            "G AMBIGUOUS_RESULT shown={} total={}",
            seeds.len(),
            proj.eligible_seed_count
        );
    }
    if ob.want_related && proj.related.len() > related.len() {
        let _ = writeln!(
            out,
            "G TRUNCATED_CONTINUATION related_shown={} related_total={}",
            related.len(),
            proj.related.len()
        );
    }
    let cand = related.iter().filter(|r| r.evidence == "candidate").count();
    let _ = writeln!(
        out,
        "S shown={} total={} complete={} seeds={} related={} candidates={} intent={}",
        seeds.len(),
        proj.eligible_seed_count,
        u8::from(proj.seed_selection_complete),
        seeds.len(),
        related.len(),
        cand,
        intent.as_str()
    );
    out
}

fn esc(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace(' ', "\\ ")
        .replace('\n', "\\n")
        .replace('\t', "\\t")
}
