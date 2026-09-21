//! Investigation-graph build: project upstream artifacts into nodes + edges.
//!
//! ```text
//! TASK 3A snapshot   -> file / declaration / import / call nodes, `contains`
//! TASK 3B links      -> entity nodes, `member_of`, structural-link edges
//! candidate artifact -> `call_candidate` edges (single / set) + dispositions
//! ```
//!
//! The builder never reparses source and never resolves anything itself — it
//! only copies already-decided relationships into graph form.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Instant;

use crate::candidates::{self, CandidateOutcome};
use crate::links::model::{FactKind, LinkRecord, LinkTarget};
use crate::repository::artifact as snapshot_artifact;
use crate::repository::manifest::RepositoryManifest;

use super::model::{
    edge_id_for, node_id_for, EvidenceClass, GraphEdge, GraphFingerprint, GraphManifest, GraphNode,
    NodeKind, GRAPH_MANIFEST_VERSION, GRAPH_POLICY_VERSION, GRAPH_SCHEMA_VERSION,
};
use super::{artifact, GraphError};

/// Wall-clock cost of each graph-build phase, in milliseconds.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GraphPhaseTimings {
    pub load_ms: f64,
    pub derive_ms: f64,
    pub serialize_ms: f64,
    pub verify_ms: f64,
}

/// Work performed by one graph build.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GraphBuildStats {
    pub files: u64,
    pub nodes: u64,
    pub edges: u64,
    pub candidate_edges: u64,
    pub candidate_sets: u64,
    pub graph_bytes: u64,
    pub duration_ms: f64,
    pub phases: GraphPhaseTimings,
}

/// Result of a successful graph build.
#[derive(Debug, Clone)]
pub struct GraphBuildOutcome {
    pub manifest: GraphManifest,
    pub stats: GraphBuildStats,
}

/// Canonical key for a node, used to derive its stable id.
fn node_key(kind: NodeKind, path: &str, fact_id: Option<u32>, entity_id: Option<&str>) -> String {
    match kind {
        NodeKind::File => format!("file:{path}"),
        NodeKind::Entity => format!("ent:{}", entity_id.unwrap_or("")),
        NodeKind::Declaration => format!("decl:{path}#{}", fact_id.unwrap_or(0)),
        NodeKind::Import => format!("imp:{path}#{}", fact_id.unwrap_or(0)),
        NodeKind::Call => format!("call:{path}#{}", fact_id.unwrap_or(0)),
    }
}

/// Build the investigation graph for `snapshot_dir` + `links_dir` +
/// `candidates_dir` into `output`.
pub fn build_graph(
    snapshot_dir: &Path,
    links_dir: &Path,
    candidates_dir: &Path,
    output: &Path,
) -> Result<GraphBuildOutcome, GraphError> {
    let started = Instant::now();
    let mut stats = GraphBuildStats::default();
    let mut phases = GraphPhaseTimings::default();

    // 1. Load upstream artifacts.
    let load_started = Instant::now();
    let snapshot_manifest: RepositoryManifest = snapshot_artifact::read_manifest(snapshot_dir)
        .map_err(|e| GraphError::Upstream {
            reason: format!("snapshot unreadable: {e}"),
        })?;
    let link_manifest =
        crate::links::artifact::read_manifest(links_dir).map_err(|e| GraphError::Upstream {
            reason: format!("link artifact unreadable: {e}"),
        })?;
    let cand_manifest =
        candidates::read_manifest(candidates_dir).map_err(|e| GraphError::Upstream {
            reason: format!("candidate artifact unreadable: {e}"),
        })?;
    // The three artifacts must describe the same snapshot.
    if link_manifest.snapshot_digest != snapshot_manifest.snapshot_digest {
        return Err(GraphError::UpstreamMismatch {
            reason: "link artifact is not built over this snapshot".to_string(),
        });
    }
    if cand_manifest.snapshot_digest != snapshot_manifest.snapshot_digest {
        return Err(GraphError::UpstreamMismatch {
            reason: "candidate artifact is not built over this snapshot".to_string(),
        });
    }
    if cand_manifest.link_digest != link_manifest.link_digest {
        return Err(GraphError::UpstreamMismatch {
            reason: "candidate artifact is not built over this link artifact".to_string(),
        });
    }
    let mut analyses = Vec::with_capacity(snapshot_manifest.files.len());
    for file in &snapshot_manifest.files {
        analyses.push(
            snapshot_artifact::read_file_analysis(snapshot_dir, file).map_err(|e| {
                GraphError::Upstream {
                    reason: format!("snapshot file `{}`: {e}", file.relative_path),
                }
            })?,
        );
    }
    let links =
        crate::links::artifact::read_links(links_dir).map_err(|e| GraphError::Upstream {
            reason: format!("link records unreadable: {e}"),
        })?;
    let entities =
        crate::links::artifact::read_entities(links_dir).map_err(|e| GraphError::Upstream {
            reason: format!("structural entities unreadable: {e}"),
        })?;
    let records = candidates::read_records(candidates_dir).map_err(|e| GraphError::Upstream {
        reason: format!("candidate records unreadable: {e}"),
    })?;
    phases.load_ms = elapsed_ms(load_started);

    // 2. Derive nodes + edges.
    let derive_started = Instant::now();
    let mut nodes: BTreeMap<String, GraphNode> = BTreeMap::new(); // key -> node
    let mut edges: Vec<GraphEdge> = Vec::new();
    let mut languages = std::collections::BTreeSet::new();

    // File + per-file fact nodes.
    for a in &analyses {
        let path = a.file.relative_path.clone();
        let lang = a.file.language.as_str().to_string();
        let file_id = add_node(
            &mut nodes,
            &mut languages,
            NodeSpec {
                key: node_key(NodeKind::File, &path, None, None),
                kind: NodeKind::File,
                language: lang.clone(),
                path: path.clone(),
                label: path.clone(),
                disposition: None,
            },
        );
        // Containment: file -> declaration/import/call.
        // Collect callable (function/method) declarations so a call can be
        // contained by its innermost enclosing callable (policy v2).
        let mut callables: Vec<(String, u32, u32)> = Vec::new();
        for d in &a.declarations {
            let id = add_node(
                &mut nodes,
                &mut languages,
                NodeSpec {
                    key: node_key(NodeKind::Declaration, &path, Some(d.declaration_id), None),
                    kind: NodeKind::Declaration,
                    language: lang.clone(),
                    path: path.clone(),
                    label: d.name.clone(),
                    disposition: None,
                },
            );
            push_edge(
                &mut edges,
                "contains",
                EvidenceClass::Fact,
                "repodex.containment",
                file_id.clone(),
                id.clone(),
                EdgeMeta::default(),
            );
            if matches!(
                d.kind,
                crate::model::DeclarationKind::Function | crate::model::DeclarationKind::Method
            ) {
                callables.push((id, d.range.byte_start, d.range.byte_end));
            }
        }
        for i in &a.imports {
            let id = add_node(
                &mut nodes,
                &mut languages,
                NodeSpec {
                    key: node_key(NodeKind::Import, &path, Some(i.import_id), None),
                    kind: NodeKind::Import,
                    language: lang.clone(),
                    path: path.clone(),
                    label: i.statement_text.clone(),
                    disposition: None,
                },
            );
            push_edge(
                &mut edges,
                "contains",
                EvidenceClass::Fact,
                "repodex.containment",
                file_id.clone(),
                id,
                EdgeMeta::default(),
            );
        }
        for c in &a.calls {
            let id = add_node(
                &mut nodes,
                &mut languages,
                NodeSpec {
                    key: node_key(NodeKind::Call, &path, Some(c.call_id), None),
                    kind: NodeKind::Call,
                    language: lang.clone(),
                    path: path.clone(),
                    label: c.callee_written.clone(),
                    disposition: None,
                },
            );
            push_edge(
                &mut edges,
                "contains",
                EvidenceClass::Fact,
                "repodex.containment",
                file_id.clone(),
                id.clone(),
                EdgeMeta::default(),
            );
            // Innermost enclosing function/method -> call containment, so
            // `callees(fn)` can reach the call sites inside it (policy v2).
            if let Some((owner, ..)) = callables
                .iter()
                .filter(|(_, s, e)| {
                    *s <= c.expression_range.byte_start && c.expression_range.byte_end <= *e
                })
                .min_by_key(|(_, s, e)| e - s)
            {
                push_edge(
                    &mut edges,
                    "contains",
                    EvidenceClass::Fact,
                    "repodex.callable_contains",
                    owner.clone(),
                    id,
                    EdgeMeta::default(),
                );
            }
        }
    }

    // Entity nodes + member_of (file -> entity).
    for entity in entities.iter() {
        let eid = add_node(
            &mut nodes,
            &mut languages,
            NodeSpec {
                key: node_key(NodeKind::Entity, "", None, Some(&entity.entity_id)),
                kind: NodeKind::Entity,
                language: entity.language.clone(),
                path: String::new(),
                label: entity.key.clone(),
                disposition: None,
            },
        );
        for file in &entity.files {
            let fkey = node_key(NodeKind::File, file, None, None);
            if let Some(fnode) = nodes.get(&fkey) {
                push_edge(
                    &mut edges,
                    "member_of",
                    EvidenceClass::Fact,
                    "repodex.package_membership",
                    fnode.node_id.clone(),
                    eid.clone(),
                    EdgeMeta::default(),
                );
            }
        }
    }

    // Structural-link edges: project each resolved link to node edges.
    for link in &links {
        project_link(link, &nodes, &mut edges);
    }

    // Candidate edges + call dispositions.
    let mut candidate_sets = 0u64;
    for record in &records {
        let call_key = node_key(
            NodeKind::Call,
            &record.source.relative_path,
            Some(record.source.fact_id),
            None,
        );
        let set_id = record.record_id.clone();
        match &record.outcome {
            CandidateOutcome::SingleCandidate { candidate } => {
                candidate_sets += 1;
                if let (Some(src), Some(tgt)) = (
                    nodes.get(&call_key),
                    decl_node(&nodes, &candidate.relative_path, candidate.declaration_id),
                ) {
                    push_edge(
                        &mut edges,
                        "call_candidate",
                        EvidenceClass::Candidate,
                        &record.rule_id,
                        src.node_id.clone(),
                        tgt.node_id.clone(),
                        EdgeMeta {
                            upstream_id: Some(set_id.clone()),
                            candidate_set_id: Some(set_id.clone()),
                            outcome: Some("single_candidate".into()),
                        },
                    );
                }
            }
            CandidateOutcome::MultipleCandidates { candidates } => {
                candidate_sets += 1;
                for candidate in candidates {
                    if let (Some(src), Some(tgt)) = (
                        nodes.get(&call_key),
                        decl_node(&nodes, &candidate.relative_path, candidate.declaration_id),
                    ) {
                        push_edge(
                            &mut edges,
                            "call_candidate",
                            EvidenceClass::Candidate,
                            &record.rule_id,
                            src.node_id.clone(),
                            tgt.node_id.clone(),
                            EdgeMeta {
                                upstream_id: Some(set_id.clone()),
                                candidate_set_id: Some(set_id.clone()),
                                outcome: Some("multiple_candidates".into()),
                            },
                        );
                    }
                }
            }
            _ => {}
        }
        // Record the disposition on the call node.
        let disp = match &record.outcome {
            CandidateOutcome::SingleCandidate { .. } => "single_candidate".to_string(),
            CandidateOutcome::MultipleCandidates { .. } => "multiple_candidates".to_string(),
            CandidateOutcome::NoCandidate { reason } => format!("no_candidate:{reason}"),
            CandidateOutcome::OutOfScope { reason } => format!("out_of_scope:{reason}"),
        };
        if let Some(node) = nodes.get_mut(&call_key) {
            node.disposition = Some(disp);
        }
    }
    // Calls with no candidate record get an explicit uncovered disposition.
    for a in &analyses {
        for c in &a.calls {
            let k = node_key(NodeKind::Call, &a.file.relative_path, Some(c.call_id), None);
            if let Some(node) = nodes.get_mut(&k) {
                if node.disposition.is_none() {
                    node.disposition = Some("no_candidate_rule".to_string());
                }
            }
        }
    }
    phases.derive_ms = elapsed_ms(derive_started);

    // 3. Count + manifest.
    let mut node_counts: BTreeMap<String, u64> = BTreeMap::new();
    for n in nodes.values() {
        *node_counts.entry(n.kind.as_str().to_string()).or_default() += 1;
    }
    let mut edge_counts: BTreeMap<String, u64> = BTreeMap::new();
    let mut candidate_edges = 0u64;
    for e in &edges {
        *edge_counts
            .entry(format!("{}:{}", e.evidence_class.as_str(), e.kind))
            .or_default() += 1;
        if e.evidence_class == EvidenceClass::Candidate {
            candidate_edges += 1;
        }
    }
    stats.files = analyses.len() as u64;
    stats.nodes = nodes.len() as u64;
    stats.edges = edges.len() as u64;
    stats.candidate_edges = candidate_edges;
    stats.candidate_sets = candidate_sets;

    let fingerprint = GraphFingerprint::current();
    let mut manifest = GraphManifest {
        graph_manifest_version: GRAPH_MANIFEST_VERSION,
        graph_schema_version: GRAPH_SCHEMA_VERSION,
        graph_policy_version: GRAPH_POLICY_VERSION,
        snapshot_digest: snapshot_manifest.snapshot_digest.clone(),
        link_digest: link_manifest.link_digest.clone(),
        candidate_digest: cand_manifest.candidate_digest.clone(),
        graph_fingerprint: fingerprint.digest.clone(),
        graph_fingerprint_text: fingerprint.text.clone(),
        graph_digest: String::new(),
        nodes: nodes.len() as u64,
        edges: edges.len() as u64,
        node_counts: node_counts.into_iter().collect(),
        edge_counts: edge_counts.into_iter().collect(),
        languages: languages.into_iter().collect(),
    };
    manifest.graph_digest = compute_graph_digest(&nodes, &edges);

    // 4. Write into staging, verify, publish.
    let serialize_started = Instant::now();
    let staging = artifact::prepare_staging(output)?;
    let node_vec: Vec<GraphNode> = nodes.into_values().collect();
    let result = artifact::write(&staging, &manifest, &node_vec, &edges);
    phases.serialize_ms = elapsed_ms(serialize_started);
    if let Err(e) = result {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(e);
    }
    let verify_started = Instant::now();
    let verified = artifact::verify(&staging, snapshot_dir, links_dir, candidates_dir);
    phases.verify_ms = elapsed_ms(verify_started);
    if let Err(e) = verified {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(e);
    }
    artifact::publish(&staging, output)?;
    stats.graph_bytes = artifact::artifact_size(output);
    stats.duration_ms = elapsed_ms(started);
    stats.phases = phases;
    Ok(GraphBuildOutcome { manifest, stats })
}

/// Resolve a declaration node by its path + declaration id.
fn decl_node<'a>(
    nodes: &'a BTreeMap<String, GraphNode>,
    path: &str,
    declaration_id: u32,
) -> Option<&'a GraphNode> {
    nodes.get(&node_key(
        NodeKind::Declaration,
        path,
        Some(declaration_id),
        None,
    ))
}

/// Project one structural link record into node edges for each resolved target.
fn project_link(
    link: &LinkRecord,
    nodes: &BTreeMap<String, GraphNode>,
    edges: &mut Vec<GraphEdge>,
) {
    let src_key = link_source_key(&link.source);
    let Some(source) = nodes.get(&src_key) else {
        return; // source fact has no node (e.g. a reference); nothing to attach.
    };
    for target in link.outcome.all_targets() {
        let Some(tgt_key) = link_target_key(target) else {
            continue;
        };
        let Some(target_node) = nodes.get(&tgt_key) else {
            continue; // dangling target — do not retain a broken locator (§31).
        };
        push_edge(
            edges,
            &link.kind,
            EvidenceClass::Fact,
            &link.rule_id,
            source.node_id.clone(),
            target_node.node_id.clone(),
            EdgeMeta {
                upstream_id: Some(link.link_id.clone()),
                candidate_set_id: None,
                outcome: Some(link.outcome.as_str().to_string()),
            },
        );
    }
}

/// The node key for a link's source fact locator.
fn link_source_key(loc: &crate::links::model::FactLocator) -> String {
    match loc.fact_kind {
        FactKind::Declaration => node_key(
            NodeKind::Declaration,
            &loc.relative_path,
            Some(loc.fact_id),
            None,
        ),
        FactKind::Import => node_key(
            NodeKind::Import,
            &loc.relative_path,
            Some(loc.fact_id),
            None,
        ),
        FactKind::Call => node_key(NodeKind::Call, &loc.relative_path, Some(loc.fact_id), None),
        // References have no graph node; their links attach nowhere.
        FactKind::Reference => String::new(),
    }
}

/// The node key for a link target, when it maps to a graph node.
fn link_target_key(target: &LinkTarget) -> Option<String> {
    match target {
        LinkTarget::File { relative_path } => {
            Some(node_key(NodeKind::File, relative_path, None, None))
        }
        LinkTarget::Declaration {
            relative_path,
            declaration_id,
            ..
        } => Some(node_key(
            NodeKind::Declaration,
            relative_path,
            Some(*declaration_id),
            None,
        )),
        LinkTarget::Structure { entity_id, .. } => {
            Some(node_key(NodeKind::Entity, "", None, Some(entity_id)))
        }
    }
}

/// Optional provenance metadata bundled for `push_edge`.
#[derive(Default)]
struct EdgeMeta {
    upstream_id: Option<String>,
    candidate_set_id: Option<String>,
    outcome: Option<String>,
}

fn push_edge(
    edges: &mut Vec<GraphEdge>,
    kind: &str,
    evidence_class: EvidenceClass,
    rule_id: &str,
    source: String,
    target: String,
    meta: EdgeMeta,
) {
    let edge_id = edge_id_for(
        kind,
        &source,
        &target,
        meta.upstream_id.as_deref().unwrap_or(""),
    );
    edges.push(GraphEdge {
        edge_id,
        kind: kind.to_string(),
        evidence_class,
        rule_id: rule_id.to_string(),
        source,
        target,
        upstream_id: meta.upstream_id,
        candidate_set_id: meta.candidate_set_id,
        outcome: meta.outcome,
    });
}

/// Fields for a node to insert (`node_id` is derived).
struct NodeSpec {
    key: String,
    kind: NodeKind,
    language: String,
    path: String,
    label: String,
    disposition: Option<String>,
}

/// Insert-or-get a node, returning its stable id.
fn add_node(
    nodes: &mut BTreeMap<String, GraphNode>,
    languages: &mut std::collections::BTreeSet<String>,
    spec: NodeSpec,
) -> String {
    languages.insert(spec.language.clone());
    let id = node_id_for(&spec.key);
    nodes.entry(spec.key.clone()).or_insert_with(|| GraphNode {
        node_id: id.clone(),
        key: spec.key,
        kind: spec.kind,
        language: spec.language,
        path: spec.path,
        label: spec.label,
        disposition: spec.disposition,
    });
    id
}

/// Deterministic digest over the whole node+edge set.
fn compute_graph_digest(nodes: &BTreeMap<String, GraphNode>, edges: &[GraphEdge]) -> String {
    let mut text = String::from("repodex-graph-v1\n");
    for n in nodes.values() {
        text.push_str(&format!(
            "n {} {} {} {}\n",
            n.node_id,
            n.kind.as_str(),
            n.key,
            n.disposition.as_deref().unwrap_or("-")
        ));
    }
    for e in edges {
        text.push_str(&format!(
            "e {} {} {} {} {}\n",
            e.edge_id,
            e.kind,
            e.evidence_class.as_str(),
            e.source,
            e.target
        ));
    }
    crate::repository::digest::sha256_text(&text)
}

fn elapsed_ms(started: Instant) -> f64 {
    started.elapsed().as_secs_f64() * 1000.0
}
