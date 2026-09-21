//! Deterministic investigation primitives over the graph index.
//!
//! Exact lookup, candidate callers/callees, bounded path search and the
//! neighborhood primitive — all preserving the FACT/CANDIDATE distinction and
//! all deterministic.

use std::collections::{BTreeSet, VecDeque};

use super::index::GraphIndex;
use super::model::{EvidenceClass, GraphEdge, GraphNode, NodeKind};

/// Which lookup domain an exact `find` searches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LookupDomain {
    /// Node id (`gn-…`) or canonical key.
    Id,
    /// Declaration/entity name (exact, case-sensitive then case-insensitive).
    Name,
    /// Exact file path or a `/`-suffix path match.
    Path,
    /// Structural entity/package/module name.
    Entity,
    /// All of the above.
    Any,
}

/// One path through the graph: an ordered list of edges (and their nodes).
#[derive(Debug, Clone)]
pub struct Path {
    /// The nodes along the path, `nodes[0]` = start.
    pub nodes: Vec<GraphNode>,
    /// The edges between consecutive nodes.
    pub edges: Vec<GraphEdge>,
    /// True if every edge is FACT.
    pub fact_only: bool,
    /// True if the path contains any CANDIDATE edge.
    pub has_candidate: bool,
    /// The candidate-set ids this path depends on (empty for fact-only).
    pub candidate_sets: Vec<String>,
}

impl Path {
    /// A precise human label for the path's evidence class (§13/§24).
    ///
    /// `structural path` = FACT edges only. Any candidate edge makes it a
    /// `candidate path` — a *possible* route, never a confirmed one. The number
    /// of distinct `candidate_sets` it traverses is exposed separately.
    pub fn evidence_label(&self) -> &'static str {
        if self.fact_only {
            "structural path"
        } else if self.candidate_sets.len() > 1 {
            "candidate path (multiple candidate sets)"
        } else {
            "candidate path"
        }
    }
}

/// Bounded path-search controls (§12/§27).
#[derive(Debug, Clone)]
pub struct PathOptions {
    pub max_depth: usize,
    pub max_paths: usize,
    pub max_nodes: usize,
    /// Restrict followed edges to these kinds (empty = all).
    pub kinds: Vec<String>,
    /// Restrict followed edges to this evidence class.
    pub evidence: Option<EvidenceClass>,
}

impl Default for PathOptions {
    fn default() -> Self {
        Self {
            max_depth: 6,
            max_paths: 16,
            max_nodes: 4096,
            kinds: Vec::new(),
            evidence: None,
        }
    }
}

impl GraphIndex {
    /// Deterministic exact lookup (§8/§15). Returns all source-grounded matches
    /// in canonical order — ambiguous names return every match, never just the
    /// first.
    pub fn find(&self, term: &str, domain: LookupDomain) -> Vec<&GraphNode> {
        let mut out: BTreeSet<usize> = BTreeSet::new();
        let by_id_or_key = |s: &str| -> Option<usize> { self.index_of(s) };
        match domain {
            LookupDomain::Id => {
                if let Some(i) = by_id_or_key(term) {
                    out.insert(i);
                }
            }
            LookupDomain::Name | LookupDomain::Entity | LookupDomain::Path | LookupDomain::Any => {
                for (i, n) in self.nodes().iter().enumerate() {
                    let hit = match domain {
                        LookupDomain::Name => {
                            (n.kind == NodeKind::Declaration || n.kind == NodeKind::Entity)
                                && n.label == term
                        }
                        LookupDomain::Entity => n.kind == NodeKind::Entity && n.label == term,
                        LookupDomain::Path => {
                            n.kind == NodeKind::File
                                && (n.path == term || n.path.ends_with(&format!("/{term}")))
                        }
                        _ => {
                            // Any: id/key, decl/entity name, file path, suffix.
                            self.index_of(term) == Some(i)
                                || (n.kind == NodeKind::Declaration && n.label == term)
                                || (n.kind == NodeKind::Entity && n.label == term)
                                || (n.kind == NodeKind::File
                                    && (n.path == term || n.path.ends_with(&format!("/{term}"))))
                        }
                    };
                    if hit {
                        out.insert(i);
                    }
                }
            }
        }
        out.into_iter().map(|i| &self.nodes()[i]).collect()
    }

    /// Internal index lookup helper.
    fn index_of(&self, id_or_key: &str) -> Option<usize> {
        self.nodes()
            .iter()
            .position(|n| n.node_id == id_or_key || n.key == id_or_key)
    }

    /// Candidate callers: incoming `call_candidate` edges into a declaration
    /// (§10). These are bounded candidates, not definite runtime callers.
    pub fn callers(&self, decl_id_or_key: &str) -> Vec<&GraphEdge> {
        self.incoming(decl_id_or_key)
            .into_iter()
            .filter(|e| e.kind == "call_candidate")
            .collect()
    }

    /// Candidate callees: the `call` nodes contained by this declaration/file
    /// (`contains` FACT) each expand to their `call_candidate` targets (§11).
    /// Returns `(call_site, candidate_edge, candidate_declaration)` triples.
    pub fn callees(&self, id_or_key: &str) -> Vec<(&GraphEdge, &GraphEdge, &GraphNode)> {
        let mut out = Vec::new();
        // contained call sites of this node (decl->call or file->call).
        for contains in self.outgoing(id_or_key) {
            if contains.kind != "contains" {
                continue;
            }
            let Some(call) = self.node(&contains.target) else {
                continue;
            };
            if call.kind != NodeKind::Call {
                continue;
            }
            for cand in self.outgoing(&call.node_id) {
                if cand.kind != "call_candidate" {
                    continue;
                }
                if let Some(decl) = self.node(&cand.target) {
                    out.push((contains, cand, decl));
                }
            }
        }
        out.sort_by(|a, b| a.1.edge_id.cmp(&b.1.edge_id));
        out
    }

    /// Bounded breadth-first path search from `from` to `to` (§12). Returns up
    /// to `options.max_paths` shortest paths in deterministic order.
    pub fn paths(&self, from: &str, to: &str, options: &PathOptions) -> Vec<Path> {
        let Some(&start) = self.index_of(from).as_ref() else {
            return Vec::new();
        };
        let Some(&goal) = self.index_of(to).as_ref() else {
            return Vec::new();
        };
        if start == goal {
            return vec![Path {
                nodes: vec![self.nodes()[start].clone()],
                edges: Vec::new(),
                fact_only: true,
                has_candidate: false,
                candidate_sets: Vec::new(),
            }];
        }
        let mut found = Vec::new();
        let mut visited_edges: BTreeSet<usize> = BTreeSet::new();
        // BFS: (node idx, path of edge idxs). Shortest paths first.
        let mut queue: VecDeque<(usize, Vec<usize>)> = VecDeque::from([(start, Vec::new())]);
        let mut best_depth = usize::MAX;
        while let Some((i, path_edges)) = queue.pop_front() {
            let depth = path_edges.len();
            if depth > best_depth || depth >= options.max_depth {
                continue;
            }
            if visited_edges.len() >= options.max_nodes || found.len() >= options.max_paths {
                break;
            }
            for e in self.outgoing(&self.nodes()[i].node_id) {
                if !options.kinds.is_empty() && !options.kinds.iter().any(|k| k == &e.kind) {
                    continue;
                }
                if let Some(ev) = options.evidence {
                    if e.evidence_class != ev {
                        continue;
                    }
                }
                let Some(&t) = self.index_of(&e.target).as_ref() else {
                    continue;
                };
                let ei = self.edge_index(&e.edge_id);
                if path_edges.contains(&ei) {
                    continue; // no repeated edge (acyclic per path)
                }
                let mut next = path_edges.clone();
                next.push(ei);
                if t == goal {
                    found.push(self.build_path(start, &next));
                    best_depth = next.len();
                    if found.len() >= options.max_paths {
                        break;
                    }
                } else if next.len() < options.max_depth {
                    visited_edges.insert(ei);
                    queue.push_back((t, next));
                }
            }
        }
        found
    }

    fn edge_index(&self, edge_id: &str) -> usize {
        self.edges()
            .iter()
            .position(|e| e.edge_id == edge_id)
            .unwrap_or(0)
    }

    fn build_path(&self, start: usize, edge_ids: &[usize]) -> Path {
        let mut nodes = vec![self.nodes()[start].clone()];
        let mut edges = Vec::new();
        let mut fact_only = true;
        let mut has_candidate = false;
        let mut sets = BTreeSet::new();
        for &ei in edge_ids {
            let e = &self.edges()[ei];
            edges.push(e.clone());
            if e.evidence_class == EvidenceClass::Candidate {
                fact_only = false;
                has_candidate = true;
                if let Some(s) = &e.candidate_set_id {
                    sets.insert(s.clone());
                }
            }
            if let Some(t) = self.node(&e.target) {
                nodes.push(t.clone());
            }
        }
        Path {
            nodes,
            edges,
            fact_only,
            has_candidate,
            candidate_sets: sets.into_iter().collect(),
        }
    }
}
