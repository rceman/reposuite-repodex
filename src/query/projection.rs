//! Canonical evidence projection (§6).
//!
//! ONE bounded projection is built from `QueryResult` (+ the validated view's
//! snapshot for current-range materialization) and every transport serializes
//! from it — service JSON, machine JSON, RDX, human — so no renderer reinterprets
//! `QueryResult` independently and no Agent-relevant field is dropped.
//!
//!   QueryResult ──► EvidenceProjection ──► { machine JSON, RDX, human }

use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;

use crate::graph::model::NodeKind;
use crate::query::engine::QueryResult;

pub const PROJECTION_SCHEMA: &str = "reposuite.repodex.query-projection.v1";

/// A current-view source range locator (row/col + byte offsets). Metadata only —
/// it is NOT a SourceExposure: the bytes were not delivered to the model (§55-56).
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct RangeOut {
    pub path: String,
    pub start_line: u32,
    pub end_line: u32,
    pub byte_start: u32,
    pub byte_end: u32,
}

/// One emitted seed with explicit rank + current-view navigation identity.
#[derive(Debug, Clone, Serialize)]
pub struct SeedOut {
    /// Explicit retrieval rank: 1 = highest-ranked emitted seed (§7). Never a
    /// local node id and never derived from canonical-key order (§8).
    pub rank: u32,
    /// Canonical locator (`decl:path#id`, `file:path`, `ent:id`, ...).
    pub key: String,
    pub kind: String,
    pub label: String,
    /// Repository-relative path (current view).
    pub path: String,
    pub language: String,
    /// Call/import disposition where present (`single_candidate`, ...).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disposition: Option<String>,
    /// Integer relevance score — ordering metadata only, not certainty.
    pub score: i64,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub factors: Vec<String>,
    /// Current declaration/header range (§11-§14), when resolvable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub declaration_range: Option<RangeOut>,
    /// Current body range, when the declaration has a body.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_range: Option<RangeOut>,
}

/// One relationship with both endpoints + provenance preserved (§15-§19).
/// `from_*`/`to_*` are direction-resolved endpoints; `node`/`label`/`via` retain
/// the legacy names (additive, backward-compatible).
#[derive(Debug, Clone, Serialize)]
pub struct RelOut {
    pub direction: String,
    pub kind: String,
    /// `fact` | `candidate` — never promoted (§17).
    pub evidence: String,
    /// The seed this edge expanded from (`via`, §16) — the anchor endpoint.
    pub via: String,
    /// The neighbor node key (legacy `node` field).
    pub node: String,
    pub label: String,
    /// Direction-resolved endpoints (§15).
    pub from_key: String,
    pub from_label: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub from_path: String,
    pub to_key: String,
    pub to_label: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub to_path: String,
    /// Upstream deterministic rule identity (§19).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule_id: Option<String>,
    /// Candidate-set identity when this edge is one member of a bounded set (§18).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidate_set: Option<String>,
    /// Neighbor disposition (candidate resolution state), when present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disposition: Option<String>,
}

/// One connector step: from -[relation]-> to with evidence + provenance (§26).
#[derive(Debug, Clone, Serialize)]
pub struct ConnectorStep {
    pub from_key: String,
    pub from_label: String,
    pub relation: String,
    pub to_key: String,
    pub to_label: String,
    /// `fact` | `candidate`.
    pub evidence: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule_id: Option<String>,
    /// Repo-relative path of the `to` endpoint (current locator).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub to_path: String,
}

/// One bounded route (a path through the graph).
#[derive(Debug, Clone, Serialize)]
pub struct Route {
    /// `structural path` | `candidate path` — precise, not a global claim.
    pub evidence_label: String,
    pub steps: Vec<ConnectorStep>,
    /// Distinct candidate sets this route depends on (empty for fact-only).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub candidate_sets: Vec<String>,
}

/// The bounded connector packet for a `paths` query (§25-§28).
#[derive(Debug, Clone, Serialize)]
pub struct ConnectorPacket {
    /// True when >=1 route was found within the bound. `found:false` means only
    /// "no route within this bounded search" — NOT global absence (§27).
    pub found: bool,
    pub routes: Vec<Route>,
    /// Deterministic bounds actually applied.
    pub max_routes: u32,
    pub max_depth: u32,
    /// Explicit no-route semantics when `found` is false.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub no_route_meaning: Option<String>,
}

/// Explicit output bounds applied (§32).
#[derive(Debug, Clone, Default, Serialize)]
pub struct Bounds {
    pub max_seeds: usize,
    pub max_related: usize,
    pub connector_max_routes: u32,
    pub connector_max_depth: u32,
}

/// The canonical projection — the single source every transport renders from.
#[derive(Debug, Clone, Default, Serialize)]
pub struct EvidenceProjection {
    pub schema: String,
    pub query: String,
    pub intent: String,
    /// Scoped completeness (§20-§21): whether ALL matching seeds were emitted
    /// within the selection bound — NOT repository-wide semantic completeness.
    pub emitted_seed_count: usize,
    pub eligible_seed_count: usize,
    /// `true` = every matching seed was emitted within the seed bound.
    pub seed_selection_complete: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub truncated_reason: Option<String>,
    /// `true` when the related-edge bound was reached (more neighbors exist).
    pub relationship_limit_reached: bool,
    // --- legacy-compatible aliases (additive; existing fields not redefined) ---
    /// Legacy: `eligible_seed_count`.
    pub total: usize,
    /// Legacy: `emitted_seed_count`.
    pub shown: usize,
    /// Legacy: `seed_selection_complete` (all matching seeds emitted in bound).
    pub complete: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub terms: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub so_query: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub so_rerank: Option<String>,
    pub seeds: Vec<SeedOut>,
    pub related: Vec<RelOut>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connector: Option<ConnectorPacket>,
    /// Optional memory composition (additive guidance; never replaces seeds).
    /// Absent when memory mode is `off` or memory is unavailable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory: Option<serde_json::Value>,
    /// Adaptive Context Compiler block: query shape + obligation ledger +
    /// budget-limited flag. Present only for `context_policy=adaptive`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<serde_json::Value>,
    pub bounds: Bounds,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<String>,
}

const MAX_SEEDS: usize = 50;
const MAX_RELATED: usize = 256;
const CONNECTOR_MAX_ROUTES: u32 = 2;
const CONNECTOR_MAX_DEPTH: u32 = 3;

fn range_out(path: &str, r: &crate::model::SourceRange) -> RangeOut {
    RangeOut {
        path: path.to_string(),
        start_line: r.row_start + 1,
        end_line: r.row_end + 1,
        byte_start: r.byte_start,
        byte_end: r.byte_end,
    }
}

/// Resolve the current declaration/body ranges of a `decl:path#id` seed from the
/// snapshot's stored `FileAnalysis` (no reparse — §11/§41). `files_dir` is the
/// index snapshot's `files/` object dir; `manifest` maps path->object_key.
struct RangeResolver {
    files_dir: std::path::PathBuf,
    /// path -> object_key (from the snapshot manifest).
    object_of: BTreeMap<String, String>,
    /// path -> loaded FileAnalysis (lazy cache).
    cache: std::cell::RefCell<BTreeMap<String, Option<crate::model::FileAnalysis>>>,
}

impl RangeResolver {
    fn open(index_dir: &Path) -> Option<Self> {
        let snap = index_dir.join("snapshot");
        let manifest = snap.join("manifest.json");
        let text = std::fs::read_to_string(manifest).ok()?;
        let m: serde_json::Value = serde_json::from_str(&text).ok()?;
        let mut object_of = BTreeMap::new();
        for f in m["files"].as_array().into_iter().flatten() {
            if let (Some(p), Some(k)) = (f["relative_path"].as_str(), f["object_key"].as_str()) {
                object_of.insert(p.to_string(), k.to_string());
            }
        }
        Some(Self {
            files_dir: snap.join("files"),
            object_of,
            cache: std::cell::RefCell::new(BTreeMap::new()),
        })
    }

    fn analysis(&self, path: &str) -> Option<crate::model::FileAnalysis> {
        if let Some(a) = self.cache.borrow().get(path) {
            return a.clone();
        }
        let key = self.object_of.get(path)?;
        let text = std::fs::read_to_string(self.files_dir.join(format!("{key}.json"))).ok()?;
        let a: Option<crate::model::FileAnalysis> = serde_json::from_str(&text).ok();
        self.cache.borrow_mut().insert(path.to_string(), a.clone());
        a
    }

    /// `decl:path#declaration_id` -> (decl range, body range) of the CURRENT bytes.
    fn decl_ranges(&self, key: &str, path: &str) -> Option<(RangeOut, Option<RangeOut>)> {
        let id: u32 = key.rsplit('#').next()?.parse().ok()?;
        let a = self.analysis(path)?;
        let d = a.declarations.iter().find(|d| d.declaration_id == id)?;
        Some((
            range_out(path, &d.range),
            d.body_range.as_ref().map(|b| range_out(path, b)),
        ))
    }
}

/// Build the canonical projection. `index_dir` (the `indexes/{key}` dir) enables
/// current-range materialization from the snapshot; pass `None` where the
/// snapshot isn't co-located (ranges are then omitted, not invented — §13).
pub fn build(result: &QueryResult, index_dir: Option<&Path>) -> EvidenceProjection {
    let resolver = index_dir.and_then(RangeResolver::open);
    let seeds: Vec<SeedOut> = result
        .seeds
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let (dr, br) = if s.node.kind == NodeKind::Declaration {
                resolver
                    .as_ref()
                    .and_then(|r| r.decl_ranges(&s.node.key, &s.node.path))
                    .map(|(d, b)| (Some(d), b))
                    .unwrap_or((None, None))
            } else {
                (None, None)
            };
            SeedOut {
                rank: (i + 1) as u32, // explicit rank, never a local id (§7-§8)
                key: s.node.key.clone(),
                kind: s.node.kind.as_str().to_string(),
                label: s.node.label.clone(),
                path: s.node.path.clone(),
                language: s.node.language.clone(),
                disposition: s.node.disposition.clone(),
                score: s.score,
                factors: s.factors.clone(),
                declaration_range: dr,
                body_range: br,
            }
        })
        .collect();

    // Direction-resolve endpoints: `via` is the seed anchor; `node` the neighbor.
    // outgoing => seed->neighbor; incoming => neighbor->seed (§15).
    let related: Vec<RelOut> = result
        .related
        .iter()
        .map(|r| {
            let outgoing = r.direction == "outgoing";
            let (from_key, from_label, from_path) = if outgoing {
                (r.via.clone(), String::new(), String::new())
            } else {
                (
                    r.node.key.clone(),
                    r.node.label.clone(),
                    r.node.path.clone(),
                )
            };
            let (to_key, to_label, to_path) = if outgoing {
                (
                    r.node.key.clone(),
                    r.node.label.clone(),
                    r.node.path.clone(),
                )
            } else {
                (r.via.clone(), String::new(), String::new())
            };
            RelOut {
                direction: r.direction.to_string(),
                kind: r.kind.clone(),
                evidence: r.evidence.as_str().to_string(),
                via: r.via.clone(),
                node: r.node.key.clone(),
                label: r.node.label.clone(),
                from_key,
                from_label,
                from_path,
                to_key,
                to_label,
                to_path,
                rule_id: r.rule_id.clone(),
                candidate_set: r.candidate_set.clone(),
                disposition: r.node.disposition.clone(),
            }
        })
        .collect();

    let connector = if result.plan.intent == crate::query::QueryIntent::Paths {
        let routes: Vec<Route> = result
            .paths
            .iter()
            .map(|p| {
                let steps = p
                    .edges
                    .iter()
                    .enumerate()
                    .map(|(i, e)| {
                        let from = p.nodes.get(i);
                        let to = p.nodes.get(i + 1);
                        ConnectorStep {
                            from_key: from.map(|n| n.key.clone()).unwrap_or_default(),
                            from_label: from.map(|n| n.label.clone()).unwrap_or_default(),
                            relation: e.kind.clone(),
                            to_key: to.map(|n| n.key.clone()).unwrap_or_default(),
                            to_label: to.map(|n| n.label.clone()).unwrap_or_default(),
                            evidence: e.evidence_class.as_str().to_string(),
                            rule_id: Some(e.rule_id.clone()),
                            to_path: to.map(|n| n.path.clone()).unwrap_or_default(),
                        }
                    })
                    .collect();
                Route {
                    evidence_label: p.evidence_label().to_string(),
                    steps,
                    candidate_sets: p.candidate_sets.clone(),
                }
            })
            .collect();
        Some(ConnectorPacket {
            found: !routes.is_empty(),
            routes,
            max_routes: CONNECTOR_MAX_ROUTES,
            max_depth: CONNECTOR_MAX_DEPTH,
            no_route_meaning: if result.paths.is_empty() {
                Some("no route found within this bounded search (depth<=3); not a global-absence claim".to_string())
            } else {
                None
            },
        })
    } else {
        None
    };

    let seed_selection_complete = result.complete;
    EvidenceProjection {
        schema: PROJECTION_SCHEMA.to_string(),
        query: result.plan.raw.clone(),
        intent: result.plan.intent.as_str().to_string(),
        emitted_seed_count: result.shown,
        eligible_seed_count: result.total,
        seed_selection_complete,
        truncated_reason: result.truncated_reason.clone(),
        relationship_limit_reached: result.related.len() >= MAX_RELATED,
        total: result.total,
        shown: result.shown,
        complete: result.complete,
        target: result.plan.target.clone(),
        terms: result.plan.terms.clone(),
        so_query: result.so_query.clone(),
        so_rerank: result.so_rerank.clone(),
        seeds,
        related,
        connector,
        memory: None,
        context: None,
        bounds: Bounds {
            max_seeds: MAX_SEEDS,
            max_related: MAX_RELATED,
            connector_max_routes: CONNECTOR_MAX_ROUTES,
            connector_max_depth: CONNECTOR_MAX_DEPTH,
        },
        diagnostics: Vec::new(),
    }
}

impl EvidenceProjection {
    /// The machine JSON body (service + `--json`). Backward-compatible superset.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::json!({}))
    }

    /// RDX v2 — the compact agent-facing protocol, extended version-cleanly
    /// (§8, §39): `D` lines give each seed an explicit `rank=` plus current
    /// path/declaration/body range; `P` lines carry bounded connector routes.
    /// Local node ids (`F<n>`) remain key-sorted and are NOT rank — rank is
    /// explicit on the `D` line. v1 decoders can still read the F/R/S lines.
    pub fn to_rdx(&self) -> String {
        use std::fmt::Write;
        let mut out = String::from("#RDX1 v2\n");
        let _ = writeln!(out, "Q {}", esc(&self.query));
        // Local ids for every node (seeds first in rank order, then related).
        // The id ordinal is NOT the rank — rank is explicit on `D` lines (§8).
        let mut lid: BTreeMap<String, u32> = BTreeMap::new();
        let mut next = 1u32;
        let mut seed_id = Vec::new();
        for s in &self.seeds {
            let id = *lid.entry(s.key.clone()).or_insert_with(|| {
                let n = next;
                next += 1;
                n
            });
            let _ = writeln!(out, "F {} {} {} {}", id, s.kind, esc(&s.key), esc(&s.label));
            seed_id.push((id, s));
        }
        for r in &self.related {
            // `r.node` is always the non-seed (neighbor) endpoint.
            let id = *lid.entry(r.node.clone()).or_insert_with(|| {
                let n = next;
                next += 1;
                n
            });
            let _ = writeln!(out, "F {} node {} {}", id, esc(&r.node), esc(&r.label));
        }
        // Seed detail lines: explicit rank + current locator/ranges.
        for (id, s) in &seed_id {
            let mut d = format!("D {} rank={} path={}", id, s.rank, esc(&s.path));
            if let Some(r) = &s.declaration_range {
                let _ = write!(d, " decl={}:{}-{}", r.start_line, r.byte_start, r.byte_end);
            }
            if let Some(r) = &s.body_range {
                let _ = write!(d, " body={}:{}-{}", r.start_line, r.byte_start, r.byte_end);
            }
            if let Some(disp) = &s.disposition {
                let _ = write!(d, " disp={}", esc(disp));
            }
            out.push_str(&d);
            out.push('\n');
        }
        // Relationships: direction-resolved endpoints + evidence + provenance.
        for r in &self.related {
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
        // Connector routes (paths intent): from -[rel/ev]-> to chains.
        if let Some(c) = &self.connector {
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
        let cand = self
            .related
            .iter()
            .filter(|r| r.evidence == "candidate")
            .count();
        let _ = writeln!(
            out,
            "S shown={} total={} complete={} seeds={} related={} candidates={}",
            self.emitted_seed_count,
            self.eligible_seed_count,
            u8::from(self.seed_selection_complete),
            self.seeds.len(),
            self.related.len(),
            cand
        );
        if let Some(r) = &self.truncated_reason {
            let _ = writeln!(out, "S truncated=1 reason={r}");
        }
        out
    }

    /// Human rendering — simpler, but never misstates rank/FACT-CANDIDATE/
    /// completeness (§37).
    pub fn to_human(&self) -> String {
        use std::fmt::Write;
        let mut out = format!(
            "query: {}\nintent: {}\nseeds: {} of {} matching ({})\n",
            self.query,
            self.intent,
            self.emitted_seed_count,
            self.eligible_seed_count,
            if self.seed_selection_complete {
                "all matching seeds emitted"
            } else {
                "truncated"
            }
        );
        for s in &self.seeds {
            let _ = writeln!(
                out,
                "  #{:<3} {:<12} {:<30} {}",
                s.rank, s.kind, s.label, s.path
            );
            if let Some(r) = &s.declaration_range {
                let _ = writeln!(
                    out,
                    "        decl {}:{}-{}",
                    r.path, r.start_line, r.end_line
                );
            }
        }
        if !self.related.is_empty() {
            let _ = writeln!(out, "related: {}", self.related.len());
            for r in self.related.iter().take(32) {
                let _ = writeln!(
                    out,
                    "  {} -[{}:{}]-> {} ({})",
                    r.from_label, r.kind, r.evidence, r.to_label, r.direction
                );
            }
        }
        if let Some(c) = &self.connector {
            if c.found {
                for rt in &c.routes {
                    let _ = write!(out, "route ({}): ", rt.evidence_label);
                    for st in &rt.steps {
                        let _ =
                            write!(out, "{} -{}-> {} ", st.from_label, st.relation, st.to_label);
                    }
                    out.push('\n');
                }
            } else {
                let _ = writeln!(out, "no route found within depth<=3 (bounded search)");
            }
        }
        out
    }
}

fn esc(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace(' ', "\\ ")
        .replace('\n', "\\n")
        .replace('\t', "\\t")
}
