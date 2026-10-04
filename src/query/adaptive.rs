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

/// A deterministic navigation plan produced BEFORE retrieval (§4-§9): the
/// typed query operation plus resolved anchor strings. Explicit flags always
/// win over natural-language classification.
#[derive(Debug, Clone)]
pub struct NavPlan {
    /// The navigation intent (for adaptive rendering + obligations).
    pub nav: NavIntent,
    /// The typed engine operation to actually execute.
    pub query_intent: crate::query::QueryIntent,
    /// Anchor for callers/callees/paths-from (extracted when not explicit).
    pub target: Option<String>,
    /// Second endpoint for a two-anchor path question.
    pub to: Option<String>,
}

/// Extract identifier-like anchor candidates from a question: tokens joined by
/// `.`/`::`/`/` and PascalCase/camelCase or `snake_case` words, minus bounded
/// stopwords. Deterministic, no model, no NLP (§9).
pub fn extract_anchors(question: &str) -> Vec<String> {
    const STOP: &[&str] = &[
        "who",
        "what",
        "how",
        "where",
        "which",
        "does",
        "is",
        "are",
        "do",
        "can",
        "could",
        "should",
        "would",
        "will",
        "the",
        "a",
        "an",
        "and",
        "or",
        "to",
        "from",
        "in",
        "on",
        "of",
        "for",
        "by",
        "it",
        "its",
        "that",
        "this",
        "reach",
        "reaches",
        "route",
        "calls",
        "call",
        "called",
        "invoke",
        "invokes",
        "uses",
        "use",
        "relate",
        "relates",
        "related",
        "between",
        "module",
        "file",
        "path",
        "paths",
        "function",
        "functions",
        "func",
        "method",
        "methods",
        "class",
        "classes",
        "struct",
        "structs",
        "test",
        "tests",
        "verify",
        "verifies",
        "control",
        "controls",
        "manifest",
        "config",
        "configuration",
        "signature",
        "depend",
        "depends",
        "dependency",
        "dependencies",
        "via",
        "initialize",
        "initializes",
        "init",
        "declare",
        "declares",
        "declared",
        "declaration",
        "definition",
        "expose",
        "exposes",
        "own",
        "owns",
        "owned",
        "owner",
        "when",
        "why",
        "show",
        "list",
        "give",
        "me",
        "tell",
        "into",
        "through",
        "across",
        "all",
        "any",
        "each",
        "edge",
        "edges",
        "node",
        "nodes",
        "directory",
        "directories",
        "repo",
        "repository",
        "code",
        "codebase",
        "project",
        "implementation",
        "detail",
        "details",
        "evidence",
        "example",
        "primary",
        "core",
        "edition",
        "whom",
        "points",
        "point",
        "references",
        "reference",
        "directly",
        "indirectly",
        "inside",
        "outside",
        "top",
        "first",
        "last",
        "most",
        "after",
        "before",
        "under",
        "over",
        "part",
        "parts",
        "layer",
        "layers",
        "entry",
        "entrypoint",
        "entrypoints",
        "drives",
        "driven",
        "flow",
        "flows",
        "trace",
        "traces",
        "step",
        "steps",
        "returns",
        "return",
        "takes",
        "accepts",
        "belong",
        "belongs",
        "contain",
        "contains",
        "cover",
        "covers",
    ];
    let mut out = Vec::new();
    for tok in
        question.split(|c: char| !(c.is_alphanumeric() || matches!(c, '.' | ':' | '_' | '/')))
    {
        let t = tok.trim_matches(|c: char| matches!(c, '.' | '/' | ':'));
        if t.len() < 2 || t.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        if STOP.contains(&t.to_lowercase().as_str()) {
            continue;
        }
        // keep token if it looks identifier-ish (contains a capital, underscore,
        // dot/:: path, or is fully lowercase-but-dotted like util.encode).
        if out.last() != Some(&t.to_string()) {
            out.push(t.to_string());
        }
    }
    out
}

/// Plan the typed retrieval operation for a navigation request (§4-§8).
/// `explicit_*` are the parsed flag/structured values — they always win.
/// `None` explicit intent + `--nav adaptive` => classify the question first,
/// then map the intent to the engine operation and extract anchors.
pub fn plan(
    question: &str,
    explicit_intent: Option<crate::query::QueryIntent>,
    explicit_target: Option<&str>,
    explicit_to: Option<&str>,
) -> NavPlan {
    use crate::query::QueryIntent as Q;
    let nav = classify(question, explicit_intent);
    let anchors = extract_anchors(question);
    // anchors only feed typed operations (callers/callees/paths); a generic
    // Find is driven by the query text, not a guessed anchor.
    let anchored = matches!(
        nav,
        NavIntent::Callers | NavIntent::Callees | NavIntent::PathOrFlow | NavIntent::Relationship
    ) || explicit_target.is_some();
    let target = explicit_target.map(|s| s.to_string()).or_else(|| {
        if anchored {
            anchors.first().cloned()
        } else {
            None
        }
    });
    let to = explicit_to.map(|s| s.to_string()).or_else(|| {
        if anchored {
            anchors.get(1).cloned()
        } else {
            None
        }
    });
    let query_intent = if let Some(q) = explicit_intent {
        q
    } else {
        match nav {
            NavIntent::Callers => {
                if target.is_some() {
                    Q::Callers
                } else {
                    Q::Find
                }
            }
            NavIntent::Callees => {
                if target.is_some() {
                    Q::Callees
                } else {
                    Q::Find
                }
            }
            NavIntent::PathOrFlow => {
                if target.is_some() && to.is_some() {
                    Q::Paths
                } else {
                    Q::Find
                }
            }
            NavIntent::Relationship => Q::Related,
            _ => Q::Find,
        }
    };
    NavPlan {
        nav,
        query_intent,
        target,
        to,
    }
}

/// Render the adaptive agent-facing RDX conditioned on the classified intent,
/// with a real serialized byte budget (§15). Relevance-filtered irrelevant
/// edges do NOT trigger a truncation gap — only relevant-but-truncated evidence
/// does. FACT/CANDIDATE preserved; no mid-line cuts.
pub fn adaptive_rdx(proj: &EvidenceProjection, intent: NavIntent) -> String {
    adaptive_rdx_biased(proj, intent, None)
}

/// Emitted-shape accounting captured while rendering the adaptive packet —
/// structured evidence for telemetry so no consumer reparses the RDX text.
#[derive(Debug, Clone, Default)]
pub struct AdaptiveRendered {
    /// Seeds actually emitted after bounds.
    pub emitted_seeds: usize,
    /// Related edges actually emitted.
    pub emitted_related: usize,
    /// Relevant related edges the selection wanted before the byte budget.
    pub relevant_total: usize,
    /// Lines dropped by the serialized byte budget.
    pub budget_dropped: usize,
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
    adaptive_rdx_observed(proj, intent, bias).0
}

/// Same renderer as [`adaptive_rdx_biased`] but also returns the emitted-shape
/// accounting used by the gap trailer — one execution, one truth (§13).
pub fn adaptive_rdx_observed(
    proj: &EvidenceProjection,
    intent: NavIntent,
    bias: Option<&crate::learning::RouteBias>,
) -> (String, AdaptiveRendered) {
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

    // ---- record-aware emission (§19-§22,§28): build lines, then emit whole
    // records within the byte budget. Never byte-slice a String; never cut a
    // record mid-line; reserve mandatory trailer space before filling body.
    const TRAILER_RESERVE: usize = 512; // bounded worst-case G+S trailer bytes
    let mut lid: BTreeMap<String, u32> = BTreeMap::new();
    let mut next = 1u32;
    for s in &seeds {
        lid.entry(s.key.clone()).or_insert_with(|| {
            let n = next;
            next += 1;
            n
        });
    }
    for r in &related {
        for k in [&r.from_key, &r.to_key] {
            lid.entry((*k).clone()).or_insert_with(|| {
                let n = next;
                next += 1;
                n
            });
        }
    }
    let seed_id = |s: &SeedOut| -> u32 { lid[&s.key] };
    let mut head = String::from("#RDX1 adaptive v2\n");
    let _ = writeln!(head, "Q {}", esc(&proj.query));
    let _ = writeln!(head, "I {}", intent.as_str());
    // body record groups, emitted in order: seed F, endpoint F, D, R, P.
    let seed_keys: BTreeSet<&String> = seeds.iter().map(|s| &s.key).collect();
    let mut f_lines: Vec<String> = Vec::new();
    for s in &seeds {
        f_lines.push(format!(
            "F {} {} {} {}\n",
            seed_id(s),
            s.kind,
            esc(&s.key),
            esc(&s.label)
        ));
    }
    let mut endpoint_emitted: BTreeSet<String> = BTreeSet::new();
    for r in &related {
        for (k, lbl) in [(&r.from_key, &r.from_label), (&r.to_key, &r.to_label)] {
            if !seed_keys.contains(k) && endpoint_emitted.insert(k.clone()) {
                f_lines.push(format!("F {} node {} {}\n", lid[k], esc(k), esc(lbl)));
            }
        }
    }
    let mut d_lines: Vec<String> = Vec::new();
    for s in &seeds {
        let mut d = format!("D {} rank={} path={}", seed_id(s), s.rank, esc(&s.path));
        if let Some(r) = &s.declaration_range {
            let _ = write!(d, " decl={}:{}-{}", r.start_line, r.byte_start, r.byte_end);
        }
        if let Some(r) = &s.body_range {
            let _ = write!(d, " body={}:{}-{}", r.start_line, r.byte_start, r.byte_end);
        }
        d.push('\n');
        d_lines.push(d);
    }
    let mut r_lines: Vec<(String, u32, u32)> = Vec::new();
    for r in &related {
        let fs = lid[&r.from_key];
        let ts = lid[&r.to_key];
        let ev = if r.evidence == "fact" { 'f' } else { 'c' };
        let mut line = format!("R {} {} {} {}", r.kind, fs, ts, ev);
        if let Some(cs) = &r.candidate_set {
            let _ = write!(line, " cs={cs}");
        }
        if let Some(rule) = &r.rule_id {
            let _ = write!(line, " rule={}", esc(rule));
        }
        line.push('\n');
        r_lines.push((line, fs, ts));
    }
    let mut p_lines: Vec<String> = Vec::new();
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
                p.push('\n');
                p_lines.push(p);
            }
            if !c.found {
                p_lines.push("P none bounded_no_route\n".into());
            }
        }
    }
    // ---- emit within budget: head + body, reserving trailer space ----
    let budget = ob.byte_budget;
    let mut out = head.clone();
    let mut emitted_f: std::collections::BTreeSet<u32> = BTreeSet::new();
    let mut budget_dropped = 0usize;
    let body_limit = budget.saturating_sub(TRAILER_RESERVE).max(head.len());
    let emit = |out: &mut String, line: &str, dropped: &mut usize| -> bool {
        if out.len() + line.len() <= body_limit {
            out.push_str(line);
            true
        } else {
            *dropped += 1;
            false
        }
    };
    for l in &f_lines {
        if emit(&mut out, l, &mut budget_dropped) {
            if let Some(id) = l
                .strip_prefix("F ")
                .and_then(|t| t.split(' ').next())
                .and_then(|t| t.parse::<u32>().ok())
            {
                emitted_f.insert(id);
            }
        }
    }
    for l in &d_lines {
        emit(&mut out, l, &mut budget_dropped);
    }
    let mut emitted_related = 0usize;
    for (l, fs, ts) in &r_lines {
        // no dangling local refs: R only emitted when both endpoint F ids are.
        if emitted_f.contains(fs) && emitted_f.contains(ts) {
            if emit(&mut out, l, &mut budget_dropped) {
                emitted_related += 1;
            }
        } else {
            budget_dropped += 1;
        }
    }
    for l in &p_lines {
        emit(&mut out, l, &mut budget_dropped);
    }
    // ---- truthful gaps from FINAL emitted state (§24-§27) ----
    let emitted_seeds = seeds.len().min(emitted_f.len());
    let mut trailer = String::new();
    if proj.seeds.is_empty() {
        let _ = writeln!(trailer, "G NO_RESULT");
    } else if proj.eligible_seed_count > emitted_seeds || !proj.seed_selection_complete {
        // selection bound reduced eligible results -> RESULT_LIMIT, not
        // identity ambiguity (§26). Ambiguity is an intent/anchor signal only.
        let _ = writeln!(
            trailer,
            "G RESULT_LIMIT shown={} eligible={}",
            emitted_seeds, proj.eligible_seed_count
        );
    }
    if intent == NavIntent::Ambiguous {
        let _ = writeln!(trailer, "G AMBIGUOUS_RESULT reason=no_confident_intent");
    }
    if proj.relationship_limit_reached {
        let _ = writeln!(trailer, "G RELATIONSHIP_LIMIT");
    }
    if ob.want_path {
        if let Some(c) = &proj.connector {
            if !c.found {
                let _ = writeln!(trailer, "G NO_ROUTE bounded");
            }
        }
    }
    let relevant_dropped = relevant_total > emitted_related || budget_dropped > 0;
    if relevant_dropped {
        let _ = writeln!(
            trailer,
            "G TRUNCATED_CONTINUATION relevant_shown={} relevant_total={}",
            emitted_related, relevant_total
        );
    }
    if let Some(tr) = &proj.truncated_reason {
        let _ = writeln!(trailer, "G UPSTREAM_TRUNCATED reason={}", esc(tr));
    }
    let cand = related
        .iter()
        .take(emitted_related)
        .filter(|r| r.evidence == "candidate")
        .count();
    // emit gap lines first, then S with the TRUE final byte count (§24).
    for line in trailer.lines() {
        if out.len() + line.len() < budget {
            out.push_str(line);
            out.push('\n');
        }
    }
    // bytes= must equal the final packet size including this S line itself —
    // iterate to the digit-width fixpoint (converges in ≤3 passes).
    let mut s_line = String::new();
    let mut bytes = out.len();
    for _ in 0..3 {
        let cand_line = format!(
            "S seeds={} eligible={} related={} rel_eligible={} candidates={} intent={} bytes={} complete={}\n",
            emitted_seeds,
            proj.eligible_seed_count,
            emitted_related,
            relevant_total,
            cand,
            intent.as_str(),
            bytes,
            u8::from(proj.seed_selection_complete && !relevant_dropped)
        );
        let b = out.len() + cand_line.len();
        s_line = cand_line;
        if b == bytes {
            break;
        }
        bytes = b;
    }
    if out.len() + s_line.len() <= budget {
        out.push_str(&s_line);
    }
    let rendered = AdaptiveRendered {
        emitted_seeds,
        emitted_related,
        relevant_total,
        budget_dropped,
    };
    (out, rendered)
}

fn esc(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace(' ', "\\ ")
        .replace('\n', "\\n")
        .replace('\t', "\\t")
}

/// Native adaptive-packet self-consistency validator (§29). Returns the list of
/// contract violations found in a rendered packet; empty = valid. Used by the
/// native gates to verify every emitted packet is well-formed.
pub fn validate_packet(rdx: &str, byte_budget: usize) -> Vec<String> {
    let mut errs = Vec::new();
    if !rdx.starts_with("#RDX1 adaptive v2\n") {
        errs.push("missing/invalid header".into());
    }
    if rdx.len() > byte_budget {
        errs.push(format!(
            "packet {} exceeds budget {}",
            rdx.len(),
            byte_budget
        ));
    }
    if std::str::from_utf8(rdx.as_bytes()).is_err() {
        errs.push("invalid UTF-8".into());
    }
    let mut fids = BTreeSet::new();
    let mut s_seen = false;
    for line in rdx.lines() {
        match line.split(' ').next() {
            Some("F") => {
                if let Some(id) = line.split(' ').nth(1).and_then(|s| s.parse::<u32>().ok()) {
                    fids.insert(id);
                }
            }
            Some("R") => {
                let v: Vec<&str> = line.split_whitespace().collect();
                if v.len() < 5 {
                    errs.push(format!("malformed R line: {line}"));
                } else if let (Ok(fs), Ok(ts)) = (v[2].parse::<u32>(), v[3].parse::<u32>()) {
                    if fs != 0 && ts != 0 && (!fids.contains(&fs) || !fids.contains(&ts)) {
                        errs.push(format!("dangling local ref in {line}"));
                    }
                }
            }
            Some("S") => {
                s_seen = true;
                // declared byte count must equal actual packet size.
                if let Some(b) = line
                    .split("bytes=")
                    .nth(1)
                    .and_then(|s| s.split(' ').next())
                    .and_then(|s| s.parse::<usize>().ok())
                {
                    if b != rdx.len() {
                        errs.push(format!("summary bytes={b} but actual {}", rdx.len()));
                    }
                }
            }
            _ => {}
        }
    }
    if !s_seen {
        errs.push("missing S summary line".into());
    }
    errs
}
