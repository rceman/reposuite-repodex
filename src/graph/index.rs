//! In-memory investigation-graph index and bounded traversal.
//!
//! Loads the artifact once and builds adjacency maps so `outgoing`/`incoming`/
//! `neighborhood`/`traverse` are O(edges-incident) rather than a full scan.
//! All traversal is bounded (`max_depth`/`max_nodes`) and deterministically
//! ordered — no unbounded DFS, identical input gives identical output.

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::path::Path;

use super::artifact;
use super::model::{EvidenceClass, GraphEdge, GraphManifest, GraphNode, NodeKind};
use super::GraphError;

/// A loaded investigation graph.
pub struct GraphIndex {
    manifest: GraphManifest,
    nodes: Vec<GraphNode>,
    by_id: HashMap<String, usize>,
    by_key: HashMap<String, usize>,
    edges: Vec<GraphEdge>,
    /// node index -> sorted edge indices that leave it.
    outgoing: HashMap<usize, Vec<usize>>,
    /// node index -> sorted edge indices that enter it.
    incoming: HashMap<usize, Vec<usize>>,
}

/// Filters for `neighborhood`/traversal.
#[derive(Debug, Clone, Default)]
pub struct NeighborFilter {
    /// Restrict to these edge kinds (empty = all).
    pub kinds: Vec<String>,
    /// Restrict to this evidence class.
    pub evidence: Option<EvidenceClass>,
    /// Restrict to this node language.
    pub language: Option<String>,
    /// Max neighbor edges to return.
    pub max_results: usize,
    /// Follow outgoing edges.
    pub outgoing: bool,
    /// Follow incoming edges.
    pub incoming: bool,
}

impl NeighborFilter {
    pub fn both() -> Self {
        Self {
            max_results: 256,
            outgoing: true,
            incoming: true,
            ..NeighborFilter::default()
        }
    }
}

/// Bounded-traversal limits (§25/§27).
#[derive(Debug, Clone)]
pub struct TraverseOptions {
    pub max_depth: usize,
    pub max_nodes: usize,
    /// Restrict which edges are followed (empty = all).
    pub kinds: Vec<String>,
    /// Restrict to this evidence class.
    pub evidence: Option<EvidenceClass>,
}

impl Default for TraverseOptions {
    fn default() -> Self {
        Self {
            max_depth: 8,
            max_nodes: 512,
            kinds: Vec::new(),
            evidence: None,
        }
    }
}

/// One neighbor relation returned by `neighborhood`.
#[derive(Debug, Clone)]
pub struct Neighbor {
    /// `outgoing` (node -> neighbor) or `incoming` (neighbor -> node).
    pub direction: &'static str,
    pub edge: GraphEdge,
    pub node: GraphNode,
}

impl GraphIndex {
    /// Load a verified graph artifact into memory.
    pub fn load(dir: &Path) -> Result<Self, GraphError> {
        let manifest = artifact::read_manifest(dir)?;
        let nodes = artifact::read_nodes(dir)?;
        let edges = artifact::read_edges(dir)?;
        Ok(Self::from_parts(manifest, nodes, edges))
    }

    /// Build an index from already-loaded parts (used by tests/audit).
    pub fn from_parts(
        manifest: GraphManifest,
        nodes: Vec<GraphNode>,
        edges: Vec<GraphEdge>,
    ) -> Self {
        let mut by_id = HashMap::with_capacity(nodes.len());
        let mut by_key = HashMap::with_capacity(nodes.len());
        for (i, n) in nodes.iter().enumerate() {
            by_id.insert(n.node_id.clone(), i);
            by_key.insert(n.key.clone(), i);
        }
        let mut outgoing: HashMap<usize, Vec<usize>> = HashMap::new();
        let mut incoming: HashMap<usize, Vec<usize>> = HashMap::new();
        for (ei, e) in edges.iter().enumerate() {
            if let Some(&s) = by_id.get(&e.source) {
                outgoing.entry(s).or_default().push(ei);
            }
            if let Some(&t) = by_id.get(&e.target) {
                incoming.entry(t).or_default().push(ei);
            }
        }
        for v in outgoing.values_mut().chain(incoming.values_mut()) {
            v.sort_by(|&a, &b| edges[a].edge_id.cmp(&edges[b].edge_id));
        }
        Self {
            manifest,
            nodes,
            by_id,
            by_key,
            edges,
            outgoing,
            incoming,
        }
    }

    pub fn manifest(&self) -> &GraphManifest {
        &self.manifest
    }

    /// Look a node up by its `gn-…` id or its canonical key.
    pub fn node(&self, id_or_key: &str) -> Option<&GraphNode> {
        if let Some(&i) = self.by_id.get(id_or_key) {
            return Some(&self.nodes[i]);
        }
        self.by_key.get(id_or_key).map(|&i| &self.nodes[i])
    }

    pub fn nodes(&self) -> &[GraphNode] {
        &self.nodes
    }

    pub fn edges(&self) -> &[GraphEdge] {
        &self.edges
    }

    /// Edges leaving `id` (deterministically ordered).
    pub fn outgoing(&self, id_or_key: &str) -> Vec<&GraphEdge> {
        self.edge_list(id_or_key, true)
    }

    /// Edges entering `id` (deterministically ordered).
    pub fn incoming(&self, id_or_key: &str) -> Vec<&GraphEdge> {
        self.edge_list(id_or_key, false)
    }

    fn edge_list(&self, id_or_key: &str, outgoing: bool) -> Vec<&GraphEdge> {
        let Some(&i) = self
            .by_id
            .get(id_or_key)
            .or_else(|| self.by_key.get(id_or_key))
        else {
            return Vec::new();
        };
        let map = if outgoing {
            &self.outgoing
        } else {
            &self.incoming
        };
        map.get(&i)
            .map(|is| is.iter().map(|&e| &self.edges[e]).collect())
            .unwrap_or_default()
    }

    /// One-hop relations around a node, separated by direction, deterministically
    /// ordered and bounded by `filter.max_results` (§53).
    pub fn neighborhood(&self, id_or_key: &str, filter: &NeighborFilter) -> Vec<Neighbor> {
        let mut out = Vec::new();
        if filter.outgoing {
            for e in self.outgoing(id_or_key) {
                if !self.matches(e, filter) {
                    continue;
                }
                if let Some(node) = self.node(&e.target) {
                    out.push(Neighbor {
                        direction: "outgoing",
                        edge: e.clone(),
                        node: node.clone(),
                    });
                }
            }
        }
        if filter.incoming {
            for e in self.incoming(id_or_key) {
                if !self.matches(e, filter) {
                    continue;
                }
                if let Some(node) = self.node(&e.source) {
                    out.push(Neighbor {
                        direction: "incoming",
                        edge: e.clone(),
                        node: node.clone(),
                    });
                }
            }
        }
        out.sort_by(|a, b| {
            (a.edge.edge_id.as_str(), a.direction).cmp(&(b.edge.edge_id.as_str(), b.direction))
        });
        out.truncate(filter.max_results);
        out
    }

    fn matches(&self, e: &GraphEdge, filter: &NeighborFilter) -> bool {
        if !filter.kinds.is_empty() && !filter.kinds.iter().any(|k| k == &e.kind) {
            return false;
        }
        if let Some(ev) = filter.evidence {
            if e.evidence_class != ev {
                return false;
            }
        }
        if let Some(lang) = &filter.language {
            let related = self.node(&e.target).or_else(|| self.node(&e.source));
            if related.map(|n| n.language.as_str()) != Some(lang.as_str()) {
                return false;
            }
        }
        true
    }

    /// Bounded breadth-first traversal from `start`. Returns the visited nodes
    /// in deterministic (BFS + edge-id) order. Never unbounded (§25).
    pub fn traverse(&self, start: &str, options: &TraverseOptions) -> Vec<GraphNode> {
        let mut visited: BTreeSet<usize> = BTreeSet::new();
        let mut order: BTreeMap<usize, Vec<usize>> = BTreeMap::new(); // depth -> node idx
        let Some(&start_i) = self.by_id.get(start).or_else(|| self.by_key.get(start)) else {
            return Vec::new();
        };
        let mut queue = VecDeque::from([(start_i, 0usize)]);
        visited.insert(start_i);
        while let Some((i, depth)) = queue.pop_front() {
            order.entry(depth).or_default().push(i);
            if visited.len() >= options.max_nodes || depth >= options.max_depth {
                continue;
            }
            if let Some(es) = self.outgoing.get(&i) {
                for &ei in es {
                    let e = &self.edges[ei];
                    if !options.kinds.is_empty() && !options.kinds.iter().any(|k| k == &e.kind) {
                        continue;
                    }
                    if let Some(ev) = options.evidence {
                        if e.evidence_class != ev {
                            continue;
                        }
                    }
                    if let Some(&t) = self.by_id.get(&e.target) {
                        if visited.insert(t) {
                            queue.push_back((t, depth + 1));
                        }
                    }
                }
            }
        }
        order
            .into_values()
            .flatten()
            .map(|i| self.nodes[i].clone())
            .collect()
    }

    /// Repository-map projection (§52): deterministic counts.
    pub fn stats(&self) -> GraphStats {
        let mut node_kinds: BTreeMap<String, u64> = BTreeMap::new();
        let mut dispositions: BTreeMap<String, u64> = BTreeMap::new();
        for n in &self.nodes {
            *node_kinds.entry(n.kind.as_str().to_string()).or_default() += 1;
            if let Some(d) = &n.disposition {
                *dispositions
                    .entry(d.split(':').next().unwrap_or(d).to_string())
                    .or_default() += 1;
            }
        }
        let mut edge_kinds: BTreeMap<String, u64> = BTreeMap::new();
        let mut candidate_edges = 0u64;
        let mut candidate_sets = BTreeSet::new();
        for e in &self.edges {
            *edge_kinds
                .entry(format!("{}:{}", e.evidence_class.as_str(), e.kind))
                .or_default() += 1;
            if e.evidence_class == EvidenceClass::Candidate {
                candidate_edges += 1;
                if let Some(s) = &e.candidate_set_id {
                    candidate_sets.insert(s.clone());
                }
            }
        }
        GraphStats {
            nodes: self.nodes.len() as u64,
            edges: self.edges.len() as u64,
            node_kinds,
            edge_kinds,
            candidate_edges,
            candidate_sets: candidate_sets.len() as u64,
            call_dispositions: dispositions,
            languages: self.manifest.languages.clone(),
        }
    }
}

/// The `graph stats` repository-map projection.
#[derive(Debug, Clone)]
pub struct GraphStats {
    pub nodes: u64,
    pub edges: u64,
    pub node_kinds: BTreeMap<String, u64>,
    pub edge_kinds: BTreeMap<String, u64>,
    pub candidate_edges: u64,
    pub candidate_sets: u64,
    pub call_dispositions: BTreeMap<String, u64>,
    pub languages: Vec<String>,
}

impl GraphStats {
    /// Count of one node kind.
    pub fn node_kind(&self, kind: NodeKind) -> u64 {
        self.node_kinds.get(kind.as_str()).copied().unwrap_or(0)
    }
}
