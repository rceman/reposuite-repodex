//! Investigation-graph artifact persistence + verification.
//!
//! Layout:
//!
//! ```text
//! <graph>/
//!   manifest.json
//!   nodes.jsonl
//!   edges.jsonl
//! ```
//!
//! `verify` re-checks the manifest, upstream digests, canonical ids, node/edge
//! references, candidate-set grouping, canonical ordering and the absence of
//! absolute paths — a stale or corrupt graph is rejected.

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use crate::repository::digest;

use super::model::*;
use super::GraphError;

pub const MANIFEST_FILE: &str = "manifest.json";
pub const NODES_FILE: &str = "nodes.jsonl";
pub const EDGES_FILE: &str = "edges.jsonl";

/// Prepare a clean staging directory beside `output`.
pub fn prepare_staging(output: &Path) -> Result<PathBuf, GraphError> {
    let parent = output.parent().unwrap_or_else(|| Path::new("."));
    let staging = parent.join(format!(
        ".{}.staging",
        output
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("graph")
    ));
    if staging.exists() {
        fs::remove_dir_all(&staging).map_err(|e| GraphError::Io {
            path: staging.clone(),
            reason: e.to_string(),
        })?;
    }
    fs::create_dir_all(&staging).map_err(|e| GraphError::Io {
        path: staging.clone(),
        reason: e.to_string(),
    })?;
    Ok(staging)
}

/// Atomically publish a verified staging dir to `output`.
pub fn publish(staging: &Path, output: &Path) -> Result<(), GraphError> {
    if output.exists() {
        fs::remove_dir_all(output).map_err(|e| GraphError::Io {
            path: output.to_path_buf(),
            reason: e.to_string(),
        })?;
    }
    fs::rename(staging, output).map_err(|e| GraphError::Io {
        path: output.to_path_buf(),
        reason: e.to_string(),
    })
}

/// Write manifest + nodes + edges into `staging`.
pub fn write(
    staging: &Path,
    manifest: &GraphManifest,
    nodes: &[GraphNode],
    edges: &[GraphEdge],
) -> Result<(), GraphError> {
    let text = serde_json::to_string_pretty(manifest).map_err(|e| GraphError::Serialize {
        reason: e.to_string(),
    })?;
    fs::write(staging.join(MANIFEST_FILE), text).map_err(|e| GraphError::Io {
        path: staging.join(MANIFEST_FILE),
        reason: e.to_string(),
    })?;
    write_jsonl(staging, NODES_FILE, nodes)?;
    write_jsonl(staging, EDGES_FILE, edges)?;
    Ok(())
}

fn write_jsonl<T: serde::Serialize>(dir: &Path, name: &str, rows: &[T]) -> Result<(), GraphError> {
    let path = dir.join(name);
    let mut file = fs::File::create(&path).map_err(|e| GraphError::Io {
        path: path.clone(),
        reason: e.to_string(),
    })?;
    for row in rows {
        let line = serde_json::to_string(row).map_err(|e| GraphError::Serialize {
            reason: e.to_string(),
        })?;
        file.write_all(line.as_bytes())
            .and_then(|_| file.write_all(b"\n"))
            .map_err(|e| GraphError::Io {
                path: path.clone(),
                reason: e.to_string(),
            })?;
    }
    Ok(())
}

fn read_jsonl<T: serde::de::DeserializeOwned>(
    dir: &Path,
    name: &str,
) -> Result<Vec<T>, GraphError> {
    let path = dir.join(name);
    let file = fs::File::open(&path).map_err(|e| GraphError::Io {
        path: path.clone(),
        reason: e.to_string(),
    })?;
    let mut out = Vec::new();
    for line in BufReader::new(file).lines() {
        let line = line.map_err(|e| GraphError::Io {
            path: path.clone(),
            reason: e.to_string(),
        })?;
        if line.trim().is_empty() {
            continue;
        }
        out.push(
            serde_json::from_str(&line).map_err(|e| GraphError::Serialize {
                reason: format!("{name}: {e}"),
            })?,
        );
    }
    Ok(out)
}

/// Read the manifest.
pub fn read_manifest(dir: &Path) -> Result<GraphManifest, GraphError> {
    let path = dir.join(MANIFEST_FILE);
    let text = fs::read_to_string(&path).map_err(|e| GraphError::Io {
        path: path.clone(),
        reason: e.to_string(),
    })?;
    serde_json::from_str(&text).map_err(|e| GraphError::Serialize {
        reason: format!("manifest: {e}"),
    })
}

/// Read all nodes.
pub fn read_nodes(dir: &Path) -> Result<Vec<GraphNode>, GraphError> {
    read_jsonl(dir, NODES_FILE)
}

/// Read all edges.
pub fn read_edges(dir: &Path) -> Result<Vec<GraphEdge>, GraphError> {
    read_jsonl(dir, EDGES_FILE)
}

/// Total artifact bytes.
pub fn artifact_size(dir: &Path) -> u64 {
    let mut total = 0;
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            if let Ok(meta) = entry.metadata() {
                if meta.is_file() {
                    total += meta.len();
                }
            }
        }
    }
    total
}

/// The canonical digest over the artifact's node/edge content.
pub fn compute_graph_digest(nodes: &[GraphNode], edges: &[GraphEdge]) -> String {
    let mut text = String::from("repodex-graph-v1\n");
    for n in nodes {
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
    digest::sha256_text(&text)
}

/// Verify a graph artifact against its upstream artifacts (§30/§51).
pub fn verify(
    dir: &Path,
    snapshot_dir: &Path,
    links_dir: &Path,
    candidates_dir: &Path,
) -> Result<(), GraphError> {
    let manifest = read_manifest(dir)?;
    if manifest.graph_manifest_version != GRAPH_MANIFEST_VERSION
        || manifest.graph_schema_version != GRAPH_SCHEMA_VERSION
        || manifest.graph_policy_version != GRAPH_POLICY_VERSION
    {
        return Err(GraphError::FingerprintMismatch {
            expected: GraphFingerprint::current().text,
            found: manifest.graph_fingerprint_text.clone(),
        });
    }
    if manifest.graph_fingerprint != GraphFingerprint::current().digest {
        return Err(GraphError::FingerprintMismatch {
            expected: GraphFingerprint::current().digest,
            found: manifest.graph_fingerprint,
        });
    }
    // Upstream digests must match exactly.
    let snap = crate::repository::artifact::read_manifest(snapshot_dir).map_err(|e| {
        GraphError::Upstream {
            reason: format!("snapshot unreadable: {e}"),
        }
    })?;
    if snap.snapshot_digest != manifest.snapshot_digest {
        return Err(GraphError::UpstreamMismatch {
            reason: "snapshot digest mismatch".to_string(),
        });
    }
    let links =
        crate::links::artifact::read_manifest(links_dir).map_err(|e| GraphError::Upstream {
            reason: format!("link artifact unreadable: {e}"),
        })?;
    if links.link_digest != manifest.link_digest {
        return Err(GraphError::UpstreamMismatch {
            reason: "link artifact digest mismatch".to_string(),
        });
    }
    let cand =
        crate::candidates::read_manifest(candidates_dir).map_err(|e| GraphError::Upstream {
            reason: format!("candidate artifact unreadable: {e}"),
        })?;
    if cand.candidate_digest != manifest.candidate_digest {
        return Err(GraphError::UpstreamMismatch {
            reason: "candidate artifact digest mismatch".to_string(),
        });
    }

    let nodes = read_nodes(dir)?;
    let edges = read_edges(dir)?;

    // Canonical ordering + no duplicate ids + no absolute paths.
    let mut node_ids = std::collections::HashSet::new();
    let mut prev_key = String::new();
    for (i, n) in nodes.iter().enumerate() {
        if !node_ids.insert(n.node_id.clone()) {
            return Err(GraphError::Corrupt {
                reason: format!("duplicate node id {}", n.node_id),
            });
        }
        if i > 0 && n.key < prev_key {
            return Err(GraphError::Corrupt {
                reason: "nodes are not in canonical key order".to_string(),
            });
        }
        prev_key = n.key.clone();
        if n.node_id != node_id_for(&n.key) {
            return Err(GraphError::Corrupt {
                reason: format!("node id {} does not match its key", n.node_id),
            });
        }
        if Path::new(&n.path).is_absolute() {
            return Err(GraphError::Corrupt {
                reason: format!("absolute path in node {}", n.node_id),
            });
        }
    }

    let mut edge_ids = std::collections::HashSet::new();
    for e in &edges {
        if !edge_ids.insert(e.edge_id.clone()) {
            return Err(GraphError::Corrupt {
                reason: format!("duplicate edge id {}", e.edge_id),
            });
        }
        if !node_ids.contains(&e.source) {
            return Err(GraphError::DanglingReference {
                reason: format!("edge {} has unknown source {}", e.edge_id, e.source),
            });
        }
        if !node_ids.contains(&e.target) {
            return Err(GraphError::DanglingReference {
                reason: format!("edge {} has unknown target {}", e.edge_id, e.target),
            });
        }
        if e.evidence_class == EvidenceClass::Candidate && e.candidate_set_id.is_none() {
            return Err(GraphError::Corrupt {
                reason: format!("candidate edge {} lacks a candidate_set_id", e.edge_id),
            });
        }
    }
    // Recompute the digest.
    let digest = compute_graph_digest(&nodes, &edges);
    if digest != manifest.graph_digest {
        return Err(GraphError::DigestMismatch {
            expected: digest,
            found: manifest.graph_digest,
        });
    }
    Ok(())
}
