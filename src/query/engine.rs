//! The deterministic textual query engine.
//!
//! ```text
//! raw query text
//!   -> QueryPlan        (typed, inspectable — §19)
//!   -> seed lookup      (deterministic lexical match over node labels/paths)
//!   -> bounded expansion (graph neighborhood of the seeds)
//!   -> deterministic ranking (integer score, documented factors)
//!   -> QueryResult      (ranked | exhaustive, with completeness)
//! ```
//!
//! No model, no embeddings, no probabilistic matching. A `ranked` query may
//! order and limit by score/budget; an `exhaustive` query never silently drops
//! a source-grounded match — truncation sets `complete=false` + `total`.

use std::collections::BTreeMap;

use crate::graph::{EvidenceClass, GraphIndex, GraphNode, NodeKind};

use super::normalize::{identifier_terms, query_terms};

/// Query mode (§20).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryMode {
    /// Order by score, may limit to a budget.
    Ranked,
    /// Return every source-grounded match; budget may truncate *presentation*
    /// but then `complete=false` and `total` stays known.
    Exhaustive,
}

/// The user's investigation intent (§25).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryIntent {
    /// Generic repository lookup + bounded neighborhood (the default).
    Find,
    /// Candidate callers of the matched declarations.
    Callers,
    /// Candidate callees reachable from the matched declarations/files.
    Callees,
    /// One-hop related entities (facts + candidates).
    Related,
    /// Bounded candidate paths between two resolved nodes.
    Paths,
}

impl QueryIntent {
    pub fn as_str(self) -> &'static str {
        match self {
            QueryIntent::Find => "find",
            QueryIntent::Callers => "callers",
            QueryIntent::Callees => "callees",
            QueryIntent::Related => "related",
            QueryIntent::Paths => "paths",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "find" | "lookup" | "symbol" => QueryIntent::Find,
            "callers" => QueryIntent::Callers,
            "callees" => QueryIntent::Callees,
            "related" | "neighborhood" => QueryIntent::Related,
            "paths" | "path" => QueryIntent::Paths,
            _ => return None,
        })
    }
}

/// A typed, inspectable deterministic query plan (§19). A future System One
/// model would produce this same structure from natural language.
#[derive(Debug, Clone)]
pub struct QueryPlan {
    /// Normalized lowercase query terms.
    pub terms: Vec<String>,
    /// The raw query text.
    pub raw: String,
    pub mode: QueryMode,
    pub intent: QueryIntent,
    /// Optional explicit target for callers/callees/paths (a name or node).
    pub target: Option<String>,
    /// For `paths`: the second endpoint.
    pub to: Option<String>,
    pub max_depth: usize,
    pub max_results: usize,
    /// Output-selection token budget (RDX1/human); never alters truth (§38).
    pub token_budget: Option<usize>,
}

impl QueryPlan {
    /// Parse raw query text + options into a plan. No natural-language intent
    /// inference — `intent`/`target` come from explicit options; free text is a
    /// generic `find`.
    pub fn parse(
        raw: &str,
        mode: QueryMode,
        intent: Option<QueryIntent>,
        target: Option<String>,
        to: Option<String>,
        max_results: usize,
        token_budget: Option<usize>,
    ) -> Self {
        Self {
            terms: query_terms(raw),
            raw: raw.to_string(),
            mode,
            intent: intent.unwrap_or(QueryIntent::Find),
            target,
            to,
            max_depth: 4,
            max_results,
            token_budget,
        }
    }
}

/// A node with its deterministic rank and the factors that produced it.
#[derive(Debug, Clone)]
pub struct ScoredNode {
    pub node: GraphNode,
    /// Integer relevance score — ordering metadata only, NOT certainty (§23).
    pub score: i64,
    /// Which scoring factors fired (for `--explain`).
    pub factors: Vec<String>,
}

/// One graph-expanded relation around a seed.
#[derive(Debug, Clone)]
pub struct RelatedHit {
    /// `incoming`/`outgoing`.
    pub direction: &'static str,
    /// The edge kind + evidence class.
    pub kind: String,
    pub evidence: EvidenceClass,
    /// The neighboring node.
    pub node: GraphNode,
    /// The seed this relation came from.
    pub via: String,
    /// The candidate-set id for candidate edges (§14).
    pub candidate_set: Option<String>,
}

/// The result of a deterministic query.
#[derive(Debug, Clone)]
pub struct QueryResult {
    pub plan: QueryPlan,
    /// Ranked seed matches (declarations/entities/files).
    pub seeds: Vec<ScoredNode>,
    /// Graph-expanded relations around the seeds.
    pub related: Vec<RelatedHit>,
    /// Total seed matches (for `complete` reporting).
    pub total: usize,
    pub shown: usize,
    pub complete: bool,
    /// `budget`/`limit` when output was truncated.
    pub truncated_reason: Option<String>,
}

/// The query engine: holds a loaded graph + a term index for fast seed lookup.
pub struct QueryEngine<'a> {
    index: &'a GraphIndex,
    /// normalized term -> node indices (built once; §48).
    term_index: BTreeMap<String, Vec<usize>>,
}

impl<'a> QueryEngine<'a> {
    pub fn new(index: &'a GraphIndex) -> Self {
        let mut term_index: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (i, n) in index.nodes().iter().enumerate() {
            // Index declarations + entities by name; files by path components.
            let text = match n.kind {
                NodeKind::Declaration | NodeKind::Entity => &n.label,
                NodeKind::File => &n.path,
                _ => continue,
            };
            for t in identifier_terms(text) {
                term_index.entry(t).or_default().push(i);
            }
        }
        Self { index, term_index }
    }

    pub fn index(&self) -> &GraphIndex {
        self.index
    }

    /// Run the query.
    pub fn run(&self, plan: &QueryPlan) -> QueryResult {
        // 1. Seed lookup.
        let mut seeds = self.seed(plan);
        // 2. Intent-specific expansion.
        let related = self.expand(plan, &seeds);
        // 3. Order + budget.
        let total = seeds.len();
        let (complete, reason) = match plan.mode {
            QueryMode::Ranked => {
                seeds.sort_by(|a, b| b.score.cmp(&a.score).then(a.node.key.cmp(&b.node.key)));
                seeds.truncate(plan.max_results);
                (true, None)
            }
            QueryMode::Exhaustive => {
                seeds.sort_by(|a, b| b.score.cmp(&a.score).then(a.node.key.cmp(&b.node.key)));
                if seeds.len() > plan.max_results {
                    seeds.truncate(plan.max_results);
                    (false, Some("limit".to_string()))
                } else {
                    (true, None)
                }
            }
        };
        let shown = seeds.len();
        QueryResult {
            plan: plan.clone(),
            seeds,
            related,
            total,
            shown,
            complete,
            truncated_reason: reason,
        }
    }

    /// Lexical seed lookup + deterministic integer scoring (§21/§22).
    fn seed(&self, plan: &QueryPlan) -> Vec<ScoredNode> {
        let mut scored: BTreeMap<usize, (i64, Vec<String>)> = BTreeMap::new();
        // An explicit target (callers/callees) resolves by name/id first.
        let mut candidate_idx: Vec<usize> = Vec::new();
        if let Some(target) = &plan.target {
            for m in self.index.find(target, crate::graph::LookupDomain::Name) {
                if let Some(i) = self.node_index(&m.node_id) {
                    candidate_idx.push(i);
                }
            }
            if candidate_idx.is_empty() {
                for m in self.index.find(target, crate::graph::LookupDomain::Any) {
                    if let Some(i) = self.node_index(&m.node_id) {
                        candidate_idx.push(i);
                    }
                }
            }
        } else {
            // Free-text: match each term against the term index.
            let mut by_node: BTreeMap<usize, Vec<String>> = BTreeMap::new();
            for term in &plan.terms {
                // exact-identifier (whole query) handled below; per-term here.
                if let Some(idxs) = self.term_index.get(term) {
                    for &i in idxs {
                        by_node.entry(i).or_default().push(term.clone());
                    }
                }
            }
            candidate_idx = by_node.keys().copied().collect();
            // record which terms matched per node for scoring
            for (i, terms) in by_node {
                scored.entry(i).or_default().1.extend(terms);
            }
        }
        for i in candidate_idx {
            let node = &self.index.nodes()[i];
            let (score, factors) =
                self.score(node, plan, &scored.get(&i).cloned().unwrap_or_default().1);
            *scored.entry(i).or_default() = (score, factors);
        }
        scored
            .into_iter()
            .map(|(i, (score, factors))| ScoredNode {
                node: self.index.nodes()[i].clone(),
                score,
                factors,
            })
            .collect()
    }

    /// Deterministic integer scoring. Documented factors (§22); relevance is
    /// ordering metadata, never certainty (§23).
    fn score(
        &self,
        node: &GraphNode,
        plan: &QueryPlan,
        matched_terms: &[String],
    ) -> (i64, Vec<String>) {
        let mut score = 0i64;
        let mut factors = Vec::new();
        let label_lc = node.label.to_lowercase();
        let node_terms = identifier_terms(&node.label);
        let raw = plan.raw.trim().to_lowercase();
        // exact identifier match
        if label_lc == raw {
            score += 1000;
            factors.push("exact".into());
        }
        // all query terms present in the identifier
        if !plan.terms.is_empty() && plan.terms.iter().all(|t| node_terms.contains(t)) {
            score += 400;
            factors.push("all_terms".into());
        }
        // per-term match
        let term_hits = plan
            .terms
            .iter()
            .filter(|t| node_terms.contains(*t))
            .count() as i64;
        if term_hits > 0 {
            score += term_hits * 100;
            factors.push(format!("term_hits={term_hits}"));
        }
        // path component match (files/decls by path)
        let path_terms = identifier_terms(&node.path);
        let path_hits = plan
            .terms
            .iter()
            .filter(|t| path_terms.contains(*t))
            .count() as i64;
        if path_hits > 0 {
            score += path_hits * 30;
            factors.push(format!("path_hits={path_hits}"));
        }
        // node-kind prior: declarations > entities > files > calls
        score += match node.kind {
            NodeKind::Declaration => 20,
            NodeKind::Entity => 15,
            NodeKind::File => 10,
            NodeKind::Import => 4,
            NodeKind::Call => 2,
        };
        let _ = matched_terms;
        (score, factors)
    }

    fn node_index(&self, node_id: &str) -> Option<usize> {
        self.index.nodes().iter().position(|n| n.node_id == node_id)
    }

    /// Bounded graph expansion around the seeds, per intent (§24/§25).
    fn expand(&self, plan: &QueryPlan, seeds: &[ScoredNode]) -> Vec<RelatedHit> {
        let mut out = Vec::new();
        let limit = plan.max_results.min(64);
        for s in seeds.iter().take(limit) {
            match plan.intent {
                QueryIntent::Callers => {
                    for e in self.index.callers(&s.node.node_id) {
                        if let Some(src) = self.index.node(&e.source) {
                            out.push(RelatedHit {
                                direction: "incoming",
                                kind: e.kind.clone(),
                                evidence: e.evidence_class,
                                node: src.clone(),
                                via: s.node.key.clone(),
                                candidate_set: e.candidate_set_id.clone(),
                            });
                        }
                    }
                }
                QueryIntent::Callees => {
                    for (_, cand, decl) in self.index.callees(&s.node.node_id) {
                        out.push(RelatedHit {
                            direction: "outgoing",
                            kind: cand.kind.clone(),
                            evidence: cand.evidence_class,
                            node: decl.clone(),
                            via: s.node.key.clone(),
                            candidate_set: cand.candidate_set_id.clone(),
                        });
                    }
                }
                QueryIntent::Find | QueryIntent::Related | QueryIntent::Paths => {
                    // one-hop neighborhood, both directions.
                    for n in self
                        .index
                        .neighborhood(&s.node.node_id, &crate::graph::NeighborFilter::both())
                    {
                        out.push(RelatedHit {
                            direction: n.direction,
                            kind: n.edge.kind.clone(),
                            evidence: n.edge.evidence_class,
                            node: n.node.clone(),
                            via: s.node.key.clone(),
                            candidate_set: n.edge.candidate_set_id.clone(),
                        });
                    }
                }
            }
        }
        out.sort_by(|a, b| {
            (a.node.key.as_str(), a.direction, a.kind.as_str()).cmp(&(
                b.node.key.as_str(),
                b.direction,
                b.kind.as_str(),
            ))
        });
        out.dedup_by(|a, b| a.node.node_id == b.node.node_id && a.kind == b.kind);
        out
    }
}
