//! RepositoryView index validity gate (§17-§19).
//!
//! ONE internal boundary decides whether a stored derived index may be reused
//! for the current target view: a content fingerprint alone is necessary but
//! not sufficient — the producers (analyzer, snapshot config, link/candidate/
//! graph policies) and the consulted metadata inputs are also validity inputs.
//!
//! The gate computes a `validity_key` = hash over every input that materially
//! affects the reusable artifacts. The index directory is keyed on that, so a
//! directory hit already implies all inputs match; a stored `validity.json`
//! records the components for diagnostics and INVALID(reason) reporting.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::parser::Analyzer;
use crate::repository::digest;
use crate::repository::fingerprint::{AnalyzerFingerprint, SnapshotConfig};

use super::manifest::ViewManifest;

pub const VALIDITY_FILE: &str = "validity.json";
pub const VALIDITY_SCHEMA: &str = "reposuite.repodex.view-validity.v1";

/// Every input that materially affects the reusable derived artifacts (§18).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidityInputs {
    pub schema: String,
    /// Source + consulted-metadata content fingerprint (ViewManifest).
    pub content_fingerprint: String,
    /// Analyzer/extraction pipeline identity.
    pub analyzer_fingerprint: String,
    /// Snapshot configuration (max_file_size, gitignore, policy, schema).
    pub snapshot_config_digest: String,
    /// Link/topology producer policy identity.
    pub link_fingerprint: String,
    /// Candidate producer policy identity.
    pub candidate_fingerprint: String,
    /// Graph producer policy identity.
    pub graph_fingerprint: String,
}

/// A bounded invalidation reason (§19). Diagnostic, not a public API error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum InvalidReason {
    SourceChanged,
    MetadataChanged,
    MetadataInventoryChanged,
    AnalyzerChanged,
    ConfigChanged,
    LinkPolicyChanged,
    CandidatePolicyChanged,
    GraphPolicyChanged,
    UpstreamArtifactChanged,
    UnstableSourceCapture,
}
impl InvalidReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SourceChanged => "SOURCE_CHANGED",
            Self::MetadataChanged => "METADATA_CHANGED",
            Self::MetadataInventoryChanged => "METADATA_INVENTORY_CHANGED",
            Self::AnalyzerChanged => "ANALYZER_CHANGED",
            Self::ConfigChanged => "CONFIG_CHANGED",
            Self::LinkPolicyChanged => "LINK_POLICY_CHANGED",
            Self::CandidatePolicyChanged => "CANDIDATE_POLICY_CHANGED",
            Self::GraphPolicyChanged => "GRAPH_POLICY_CHANGED",
            Self::UpstreamArtifactChanged => "UPSTREAM_ARTIFACT_CHANGED",
            Self::UnstableSourceCapture => "UNSTABLE_SOURCE_CAPTURE",
        }
    }
}

impl ValidityInputs {
    /// Collect the current producer/config identities for `analyzer` + the
    /// content fingerprint of `manifest`.
    pub fn current(analyzer: &Analyzer, manifest: &ViewManifest) -> Self {
        let analyzer_fp = AnalyzerFingerprint::current(analyzer.registry());
        let config = SnapshotConfig::new(analyzer.config().max_file_size, true);
        Self {
            schema: VALIDITY_SCHEMA.to_string(),
            content_fingerprint: manifest.fingerprint(),
            analyzer_fingerprint: analyzer_fp.digest,
            snapshot_config_digest: digest::content_digest(
                serde_json::to_string(&config)
                    .unwrap_or_default()
                    .as_bytes(),
            ),
            link_fingerprint: crate::links::model::LinkFingerprint::current().digest,
            candidate_fingerprint: crate::candidates::model::CandidateFingerprint::current().digest,
            graph_fingerprint: crate::graph::model::GraphFingerprint::current().digest,
        }
    }

    /// The directory/validity key — a hit implies all inputs matched (§17).
    pub fn key(&self) -> String {
        digest::content_digest(serde_json::to_string(self).unwrap_or_default().as_bytes())
    }

    /// Persist the recorded inputs inside a published index for diagnostics.
    pub fn write(&self, index_dir: &Path) -> Result<(), String> {
        let p = index_dir.join(VALIDITY_FILE);
        fs::create_dir_all(index_dir).map_err(|e| e.to_string())?;
        let tmp = p.with_extension("json.tmp");
        fs::write(
            &tmp,
            serde_json::to_string_pretty(self).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        fs::rename(&tmp, &p).map_err(|e| e.to_string())
    }

    /// Read the recorded inputs of a stored index, if present.
    pub fn read(index_dir: &Path) -> Option<Self> {
        let t = fs::read_to_string(index_dir.join(VALIDITY_FILE)).ok()?;
        serde_json::from_str(&t).ok()
    }

    /// Compare a stored index's inputs against the current target view's inputs;
    /// returns `None` when VALID, or `Some(reason)` when INVALID (§17-§19).
    pub fn validate(stored: &ValidityInputs, current: &ValidityInputs) -> Option<InvalidReason> {
        if stored.analyzer_fingerprint != current.analyzer_fingerprint {
            return Some(InvalidReason::AnalyzerChanged);
        }
        if stored.snapshot_config_digest != current.snapshot_config_digest {
            return Some(InvalidReason::ConfigChanged);
        }
        if stored.link_fingerprint != current.link_fingerprint {
            return Some(InvalidReason::LinkPolicyChanged);
        }
        if stored.candidate_fingerprint != current.candidate_fingerprint {
            return Some(InvalidReason::CandidatePolicyChanged);
        }
        if stored.graph_fingerprint != current.graph_fingerprint {
            return Some(InvalidReason::GraphPolicyChanged);
        }
        if stored.content_fingerprint != current.content_fingerprint {
            return Some(InvalidReason::SourceChanged);
        }
        None
    }
}

pub fn index_dir(state: &Path, validity_key: &str) -> PathBuf {
    state.join("indexes").join(&validity_key[7..23])
}
