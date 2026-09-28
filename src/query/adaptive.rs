//! Adaptive evidence navigation (RepoDex-first, V1).
//!
//! Deterministic intent classification + obligation-aware evidence-unit
//! selection + semantic dedup + serialized-byte budget -> compact adaptive RDX.
//! Intent is routing metadata only — it never upgrades FACT/CANDIDATE (§9,§19).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

use super::projection::{EvidenceProjection, RelOut, SeedOut};

/// Repository-navigation intents (§8). Routing metadata only — never repo truth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavIntent {
    Locate,
    Definition,
    Callers,
    Callees,
    Relationship,
    PathOrFlow,
    ManifestConfig,
    TestEvidence,
    GeneralStructural,
    /// No confident cue and the query is not a clear structural question.
    /// Reachable: a question with no actionable signal returns bounded evidence
    /// plus an explicit ambiguity gap — never fabricated certainty.
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

/// Canonical call-evidence relation kinds (call/ref edges, not structural).
const CALL_KINDS: &[&str] = &["call_candidate", "call", "calls", "call_ref"];
/// Structural (non-call) kinds — never emitted as caller/callee evidence.
const STRUCTURAL_KINDS: &[&str] = &[
    "contains",
    "member_of",
    "owned_by_manifest",
    "imports",
    "declares",
];

fn is_call_edge(r: &RelOut) -> bool {
    CALL_KINDS.iter().any(|k| r.kind.starts_with(k)) && !r.kind.contains("candidate_set_only")
}

/// Deterministic cue-based classification. No model. Precedence is explicit:
/// the most specific call intent wins; generic words like "call" do NOT steal
/// a TestEvidence / Relationship / Callers question.
pub fn classify(text: &str, plan_intent: Option<crate::query::QueryIntent>) -> NavIntent {
    use crate::query::QueryIntent as Q;
    if let Some(q) = plan_intent {
        return match q {
            Q::Callers => NavIntent::Callers,
            Q::Callees => NavIntent::Callees,
            Q::Paths => NavIntent::PathOrFlow,
            Q::Related => NavIntent::Relationship,
            Q::Find => NavIntent::Locate,
        };
    }
    let t = text.to_lowercase();
    let has = |w: &[&str]| w.iter().any(|s| t.contains(s));
    // TEST first: "which tests call X" / "tests for X" must not be misrouted to
    // Callers/Callees on the bare word "call".
    if has(&["test", "tests", "covers", "cover"]) {
        return NavIntent::TestEvidence;
    }
    // caller direction: "who calls X", "who invokes X".
    if has(&[
        "who calls",
        "callers of",
        "called by",
        "who invokes",
        "call sites of",
        "who calls whom",
    ]) {
        return NavIntent::Callers;
    }
    // path/flow between two anchors.
    if has(&[
        "reach",
        "path to",
        "path from",
        "route",
        "flow",
        "trace the call",
        "how does",
    ]) {
        return NavIntent::PathOrFlow;
    }
    // explicit relationship between two anchors.
    if has(&[
        "relat",
        "between",
        "who calls whom",
        "how does .* relate",
        "depend on",
        "depends on",
    ]) {
        return NavIntent::Relationship;
    }
    // manifest/config BEFORE callees: "what does go.mod declare" is manifest.
    if has(&[
        "manifest",
        "cargo.toml",
        "go.mod",
        "config",
        "dependency",
        "does go.mod",
        "module does",
        "edition",
        "package name",
    ]) {
        return NavIntent::ManifestConfig;
    }
    // callee direction needs an explicit call-directed word — "does X call",
    // "which functions does X call", "callees", "calls", "invokes", "uses".
    // A bare "does"/"declare" alone is NOT callee evidence.
    if has(&[
        "callees",
        "functions does",
        "does ",
        " call",
        "calls ",
        " invoke",
        "uses ",
        "which functions",
        "what does",
        "functions it calls",
    ]) && (has(&["call", "invoke", "uses", "callees"]))
        && !has(&["who calls", "called by"])
    {
        return NavIntent::Callees;
    }
    if has(&[
        "signature",
        "declaration",
        "what is",
        "type of",
        "definition",
        "declares",
    ]) {
        return NavIntent::Definition;
    }
    if has(&[
        "where is",
        "find",
        "locate",
        "which file",
        "what file",
        "which module",
    ]) {
        return NavIntent::Locate;
    }
    // clear structural intent
    if has(&[
        "structure",
        "architecture",
        "overview",
        "layout",
        "organize",
        "breakdown",
    ]) {
        return NavIntent::GeneralStructural;
    }
    // No confident cue + not a clear structural question -> Ambiguous (real,
    // reachable under this rule). Never fabricate certainty.
    NavIntent::Ambiguous
}

/// Evidence-obligation set: which units + bounds an intent needs (§10).
struct Obligation {
    max_seeds: usize,
    max_related: usize,
    want_related: bool,
    want_path: bool,
    /// serialized byte budget for the final packet (§15).
    byte_budget: usize,
}

fn obligation(intent: NavIntent) -> Obligation {
    let (ms, mr, wr, wp, bb) = match intent {
        // lookup/definition <= 2 KiB
        NavIntent::Locate | NavIntent::Definition => (3, 0, false, false, 2048),
        // relationship / manifest / test <= 4 KiB
        NavIntent::Callers | NavIntent::Callees | NavIntent::Relationship => {
            (3, 16, true, false, 4096)
        }
        NavIntent::ManifestConfig => (4, 8, true, false, 4096),
        NavIntent::TestEvidence => (4, 12, true, false, 4096),
        // path / flow <= 8 KiB
        NavIntent::PathOrFlow => (2, 8, true, true, 8192),
        NavIntent::GeneralStructural | NavIntent::Ambiguous => (5, 10, true, false, 4096),
    };
    Obligation {
        max_seeds: ms,
        max_related: mr,
        want_related: wr,
        want_path: wp,
        byte_budget: bb,
    }
}

/// Is this edge relevant to the intent's evidence obligation? Call edges are
/// selected by relation semantics + direction, NOT merely by direction (§7,§8).
fn edge_relevant(r: &RelOut, intent: NavIntent) -> bool {
    let incoming = r.direction == "incoming";
    match intent {
        NavIntent::Locate | NavIntent::Definition => false,
        NavIntent::Callers => incoming && is_call_edge(r),
        NavIntent::Callees => !incoming && is_call_edge(r),
        NavIntent::Relationship => true, // both directions between anchors
        NavIntent::PathOrFlow => true,
        NavIntent::ManifestConfig => {
            // only real ownership/membership/manifest structural edges
            STRUCTURAL_KINDS.iter().any(|k| r.kind.starts_with(k)) || r.kind.contains("manifest")
        }
        NavIntent::TestEvidence => true,
        NavIntent::GeneralStructural | NavIntent::Ambiguous => true,
    }
}

/// Dedup key: kind + direction-resolved endpoints + rule + candidate set.
/// Distinct witnesses are kept; pure repeats collapse.
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

/// Render the adaptive agent-facing RDX conditioned on the classified intent,
/// with a real serialized byte budget (§15). Relevance-filtered irrelevant
/// edges do NOT trigger a truncation gap — only relevant-but-truncated evidence
/// does. FACT/CANDIDATE preserved; no mid-line cuts.
pub fn adaptive_rdx(proj: &EvidenceProjection, intent: NavIntent) -> String {
    adaptive_rdx_biased(proj, intent, None)
}

/// Adaptive RDX with an optional internal route bias from learned memory. The
/// bias only *reorders* which current relations/seeds are surfaced first — it
/// changes nothing about FACT/CANDIDATE, never adds Agent-visible text, and
/// never emits historical data. Same evidence set, better ordering.
pub fn adaptive_rdx_biased(
    proj: &EvidenceProjection,
    intent: NavIntent,
    bias: Option<&crate::learning::RouteBias>,
) -> String {
    let ob = obligation(intent);
    // Seed order: learned-preferred anchors surface first (internal reordering).
    let mut seed_order: Vec<&SeedOut> = proj.seeds.iter().collect();
    if let Some(b) = bias {
        if !b.prefer_anchors.is_empty() {
            seed_order.sort_by_key(|s| {
                if b.prefer_anchors
                    .iter()
                    .any(|a| s.key.contains(a) || s.label == *a)
                {
                    0
                } else {
                    1
                }
            });
        }
    }
    let seeds: Vec<&SeedOut> = seed_order.into_iter().take(ob.max_seeds).collect();
    // Relevance-filter + dedup. Track how many *relevant* edges were dropped by
    // the bound (truncation) vs irrelevant (filtered — not a gap).
    let mut seen = BTreeSet::new();
    let mut relevant_total = 0usize;
    let mut relevant: Vec<&RelOut> = Vec::new();
    if ob.want_related {
        for r in &proj.related {
            if !edge_relevant(r, intent) {
                continue; // IRRELEVANT_FILTERED — not exposed to the Agent.
            }
            relevant_total += 1;
            if seen.insert(rel_key(r)) {
                relevant.push(r);
            }
        }
    }
    // Internal route bias: learned-productive relation kinds surface first —
    // answer-bearing evidence earlier in the bounded packet (§13-§14).
    if let Some(b) = bias {
        if !b.prefer_kinds.is_empty() {
            relevant.sort_by_key(|r| {
                if b.prefer_kinds
                    .iter()
                    .any(|k| r.kind.starts_with(k.as_str()))
                {
                    0
                } else {
                    1
                }
            });
        }
    }
    let related: Vec<&RelOut> = relevant.into_iter().take(ob.max_related).collect();

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
        // one canonical line per Agent-relevant node identity (§14 dedup).
        let _ = writeln!(out, "F {} {} {} {}", id, s.kind, esc(&s.key), esc(&s.label));
        seed_ids.push((id, s));
    }
    for r in &related {
        for k in [&r.from_key, &r.to_key] {
            if !lid.contains_key(k.as_str()) {
                let id = next;
                next += 1;
                lid.insert((*k).clone(), id);
                let lbl = if *k == r.from_key {
                    &r.from_label
                } else {
                    &r.to_label
                };
                let _ = writeln!(out, "F {} node {} {}", id, esc(k), esc(lbl));
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
    // ---- serialized byte budget (§15): enforce on the whole packet ----
    let mut truncated_by_budget = false;
    if out.len() > ob.byte_budget {
        // cut at the last complete line that fits the budget — never mid-line.
        let keep = out[..ob.byte_budget]
            .rfind('\n')
            .map(|i| i + 1)
            .unwrap_or(ob.byte_budget);
        out.truncate(keep);
        truncated_by_budget = true;
    }
    // ---- explicit gaps (§20): only real gaps, not irrelevant-filtered ----
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
    if intent == NavIntent::Ambiguous {
        let _ = writeln!(out, "G AMBIGUOUS_RESULT reason=no_confident_intent");
    }
    // Truncation only when relevant evidence exceeded the bound or byte budget.
    let relevant_shown = related.len();
    if truncated_by_budget || relevant_total > relevant_shown {
        let _ = writeln!(
            out,
            "G TRUNCATED_CONTINUATION relevant_shown={} relevant_total={}{}",
            relevant_shown,
            relevant_total,
            if truncated_by_budget {
                " over_byte_budget=1"
            } else {
                ""
            }
        );
    }
    let cand = related.iter().filter(|r| r.evidence == "candidate").count();
    let _ = writeln!(
        out,
        "S shown={} total={} complete={} seeds={} related={} candidates={} intent={} bytes={}",
        seeds.len(),
        proj.eligible_seed_count,
        u8::from(proj.seed_selection_complete),
        seeds.len(),
        relevant_shown,
        cand,
        intent.as_str(),
        out.len()
    );
    out
}

fn esc(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace(' ', "\\ ")
        .replace('\n', "\\n")
        .replace('\t', "\\t")
}
