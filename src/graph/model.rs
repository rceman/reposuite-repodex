//! The repository investigation graph model.
//!
//! The graph is a **projection of existing evidence** — TASK 3A normalized
//! facts, TASK 3B structural links and the Rust/Go candidate artifacts — into a
//! traversable node/edge form. It is never a second parser or resolver, and it
//! never collapses bounded candidate evidence into semantic truth.
//!
//! Every edge carries an [`EvidenceClass`]:
//!
//! ```text
//! FACT       -> containment, membership, structural links (imports, use_path)
//! CANDIDATE  -> bounded call candidates (single or one member of a set)
//! ```
//!
//! A `call_candidate` edge is *not* an exact call edge. A `SingleCandidate`
//! produces one edge whose cardinality metadata records that the bounded rule
//! yielded one candidate; `MultipleCandidates` produce several edges sharing a
//! `candidate_set_id`. `NoCandidate`/`OutOfScope` produce no target edge — the
//! call node carries the disposition instead.

use serde::{Deserialize, Serialize};

use crate::repository::digest;

/// Graph record schema version.
/// * `1` — initial artifact.
/// * `2` — the canonical graph digest also covers node `path`/`label` and edge
///   `rule_id`/`upstream_id`/`candidate_set_id` so tampering is detected.
pub const GRAPH_SCHEMA_VERSION: u32 = 2;
/// Graph artifact manifest format version.
pub const GRAPH_MANIFEST_VERSION: u32 = 1;
/// Graph *policy* version — the projection rules that map upstream artifacts to
/// nodes/edges. Bump when the projection semantics change.
///
/// * `1` — file/entity/declaration/import/call nodes; `contains`/`member_of`/
///   structural-link FACT edges; `call_candidate` CANDIDATE edges with
///   `candidate_set_id` grouping; `NoCandidate`/`OutOfScope` kept as node
///   dispositions.
/// * `2` — added `function/method -> call` `contains` edges via range
///   containment, so `callees(fn)` can reach the call sites inside a callable.
pub const GRAPH_POLICY_VERSION: u32 = 2;

/// A graph node kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    /// A source file in the repository.
    File,
    /// A structural entity (Go package, Rust module/crate, PHP namespace, ...).
    Entity,
    /// A source-written declaration (function, type, var, ...).
    Declaration,
    /// An import occurrence.
    Import,
    /// A call-shaped occurrence — a call site, distinct from its candidates.
    Call,
}

impl NodeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            NodeKind::File => "file",
            NodeKind::Entity => "entity",
            NodeKind::Declaration => "declaration",
            NodeKind::Import => "import",
            NodeKind::Call => "call",
        }
    }
}

/// The two broad evidence classes every edge belongs to (§4/§22).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceClass {
    /// A source-backed or structurally-proven relationship.
    Fact,
    /// A bounded syntactic candidate — never an exact/resolved relationship.
    Candidate,
}

impl EvidenceClass {
    pub fn as_str(self) -> &'static str {
        match self {
            EvidenceClass::Fact => "fact",
            EvidenceClass::Candidate => "candidate",
        }
    }
}

/// A graph node. References upstream facts by their canonical locator key, not
/// by re-embedding the whole record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphNode {
    /// Content-derived stable id (`gn-…`).
    pub node_id: String,
    /// Canonical locator key (`file:path`, `decl:path#id`, `ent:id`, ...).
    pub key: String,
    pub kind: NodeKind,
    pub language: String,
    /// Repository-relative path of the underlying fact ("" for entities).
    pub path: String,
    /// Human-readable label (declaration name, file path, entity key, ...).
    pub label: String,
    /// For call nodes, the bounded candidate disposition
    /// (`single_candidate`/`multiple_candidates`/`no_candidate:<reason>`/
    /// `out_of_scope:<reason>`). For import nodes, the import link outcome.
    /// Absent for node kinds with no disposition.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disposition: Option<String>,
}

/// Derive a stable node id from the canonical key.
pub fn node_id_for(key: &str) -> String {
    let digest = digest::sha256_text(&format!("repodex-graph-node-v1\n{key}"));
    let hex = digest.strip_prefix("sha256:").unwrap_or(&digest);
    format!("gn-{}", &hex[..16])
}

/// A graph edge between two nodes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphEdge {
    /// Content-derived stable id (`ge-…`).
    pub edge_id: String,
    /// Relationship kind: `contains`, `member_of`, a structural link kind
    /// (`import_path`, `use_path`, ...), or `call_candidate`.
    pub kind: String,
    /// `fact` or `candidate` — the two evidence classes (§4/§22).
    pub evidence_class: EvidenceClass,
    /// The upstream rule that produced this relationship.
    pub rule_id: String,
    /// Source node id.
    pub source: String,
    /// Target node id.
    pub target: String,
    /// Upstream record id this edge projects (`lnk-…`/`cand-…`), when there is
    /// one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream_id: Option<String>,
    /// For `call_candidate` edges, the candidate-set the edge belongs to (the
    /// upstream record id). Single and multiple candidates both share it; two+
    /// edges with the same set id are one ambiguous set, not independent edges.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_set_id: Option<String>,
    /// Compact upstream outcome metadata (`exact`, `ambiguous`, the candidate
    /// cardinality, a suppression reason, ...).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<String>,
}

/// Derive a stable edge id from the relationship's canonical content.
pub fn edge_id_for(kind: &str, source_key: &str, target_key: &str, salt: &str) -> String {
    let digest = digest::sha256_text(&format!(
        "repodex-graph-edge-v1\n{kind}\n{source_key}\n{target_key}\n{salt}"
    ));
    let hex = digest.strip_prefix("sha256:").unwrap_or(&digest);
    format!("ge-{}", &hex[..16])
}

/// The derived-graph compatibility fingerprint — covers only what changes
/// graph semantics (schema/policy versions), not upstream digests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphFingerprint {
    pub text: String,
    pub digest: String,
}

impl GraphFingerprint {
    pub fn current() -> Self {
        let text = format!(
            "manifest={} schema={} policy={}",
            GRAPH_MANIFEST_VERSION, GRAPH_SCHEMA_VERSION, GRAPH_POLICY_VERSION,
        );
        Self {
            digest: digest::sha256_text(&format!("repodex-graph-fingerprint\n{text}")),
            text,
        }
    }
}

/// The graph artifact manifest — records the exact upstream identities it was
/// built over, so a stale upstream invalidates the graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphManifest {
    pub graph_manifest_version: u32,
    pub graph_schema_version: u32,
    pub graph_policy_version: u32,
    /// Upstream dependency identities.
    pub snapshot_digest: String,
    pub link_digest: String,
    pub candidate_digest: String,
    pub graph_fingerprint: String,
    pub graph_fingerprint_text: String,
    pub graph_digest: String,
    pub nodes: u64,
    pub edges: u64,
    /// Node/edge counts by kind, for `graph stats`.
    pub node_counts: Vec<(String, u64)>,
    pub edge_counts: Vec<(String, u64)>,
    pub languages: Vec<String>,
}
