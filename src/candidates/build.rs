//! Derived call-candidate build.
//!
//! ```text
//! TASK 3A snapshot
//!     -> load manifest + normalized file facts
//!     -> load the TASK 3B link manifest (dependency only)
//!     -> apply the bounded Rust call-candidate rule
//!     -> order, count, digest
//!     -> verify in staging, then publish atomically
//! ```
//!
//! The build never mutates the snapshot or the link artifact, never reparses
//! source through Tree-sitter, and never writes a second copy of the normalized
//! facts. The TASK 3B link artifact is a *dependency*: it is digested into the
//! manifest so a stale upstream invalidates this artifact, and TASK 3D reads its
//! `use_path` relationships so a blocking `use` can become an imported
//! `function` candidate.
//!
//! TASK 3C rebuilds the whole candidate artifact from the whole snapshot. The
//! measured cost (see `docs/TASK3C_RESULTS.md`) does not justify incremental
//! candidate patching yet.

use std::path::Path;
use std::time::Instant;

use crate::model::FileAnalysis;
use crate::repository::artifact as snapshot_artifact;
use crate::repository::manifest::RepositoryManifest;

use super::artifact::{self, CandidateError};
use super::model::{
    candidate_rule_registry, CallCandidateRecord, CandidateFingerprint, CandidateManifest,
    CardinalityCounts, CANDIDATE_MANIFEST_VERSION, CANDIDATE_RULE_ABI_VERSION,
    CANDIDATE_SCHEMA_VERSION,
};
use super::rule_rust;

/// Wall-clock cost of each candidate-build phase, in milliseconds.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CandidatePhaseTimings {
    pub snapshot_load_ms: f64,
    pub derive_ms: f64,
    pub serialize_ms: f64,
    pub verify_ms: f64,
}

/// Work actually performed by one candidate build.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CandidateBuildStats {
    pub snapshot_files: u64,
    pub records_total: u64,
    pub cardinalities: CardinalityCounts,
    pub snapshot_bytes: u64,
    pub link_bytes: u64,
    pub candidate_bytes: u64,
    pub duration_ms: f64,
    pub phases: CandidatePhaseTimings,
}

impl CandidateBuildStats {
    /// Candidate artifact bytes as a fraction of the snapshot artifact bytes.
    pub fn size_ratio(&self) -> f64 {
        if self.snapshot_bytes == 0 {
            0.0
        } else {
            self.candidate_bytes as f64 / self.snapshot_bytes as f64
        }
    }

    /// Candidate artifact bytes as a fraction of the link artifact bytes.
    pub fn link_ratio(&self) -> f64 {
        if self.link_bytes == 0 {
            0.0
        } else {
            self.candidate_bytes as f64 / self.link_bytes as f64
        }
    }
}

/// Result of a successful candidate build.
#[derive(Debug, Clone)]
pub struct CandidateBuildOutcome {
    pub manifest: CandidateManifest,
    pub stats: CandidateBuildStats,
}

/// Build the derived call-candidate artifact for `snapshot_dir` into `output`.
///
/// `links_dir` is the TASK 3B link artifact this build depends on. Its digest is
/// recorded so a stale upstream invalidates this artifact; the rule reads its
/// `use_path` relationships so a blocking `use` can become an imported
/// `function` candidate.
pub fn build_candidates(
    snapshot_dir: &Path,
    links_dir: &Path,
    output: &Path,
) -> Result<CandidateBuildOutcome, CandidateError> {
    let started = Instant::now();
    let mut stats = CandidateBuildStats::default();
    let mut phases = CandidatePhaseTimings::default();

    // 1. Load the snapshot and the TASK 3B link manifest.
    let load_started = Instant::now();
    let snapshot_manifest: RepositoryManifest = snapshot_artifact::read_manifest(snapshot_dir)
        .map_err(|error| CandidateError::InvalidManifest {
            reason: format!("the snapshot could not be read: {error}"),
        })?;
    let link_manifest = crate::links::artifact::read_manifest(links_dir).map_err(|error| {
        CandidateError::InvalidManifest {
            reason: format!("the link artifact could not be read: {error}"),
        }
    })?;
    if link_manifest.snapshot_digest != snapshot_manifest.snapshot_digest {
        return Err(CandidateError::SnapshotMismatch {
            expected: snapshot_manifest.snapshot_digest,
            found: link_manifest.snapshot_digest,
        });
    }
    let mut analyses: Vec<FileAnalysis> = Vec::with_capacity(snapshot_manifest.files.len());
    for file in &snapshot_manifest.files {
        analyses.push(
            snapshot_artifact::read_file_analysis(snapshot_dir, file).map_err(|error| {
                CandidateError::InvalidManifest {
                    reason: format!("snapshot artifact `{}`: {error}", file.relative_path),
                }
            })?,
        );
    }
    stats.snapshot_files = analyses.len() as u64;
    stats.snapshot_bytes = snapshot_artifact::artifact_size(snapshot_dir);
    stats.link_bytes = crate::links::artifact::artifact_size(links_dir);
    // TASK 3D reads the persisted `use_path` relationships so a blocking `use`
    // can be resolved into imported `function` candidates.
    let links = crate::links::artifact::read_links(links_dir).map_err(|error| {
        CandidateError::InvalidManifest {
            reason: format!("the link records could not be read: {error}"),
        }
    })?;
    phases.snapshot_load_ms = elapsed_ms(load_started);

    // 2. Apply the bounded Rust candidate rule.
    let derive_started = Instant::now();
    let mut records: Vec<CallCandidateRecord> = rule_rust::candidates(&analyses, &links);
    artifact::order_records(&mut records)?;
    phases.derive_ms = elapsed_ms(derive_started);

    // 3. Count.
    let mut cardinalities = CardinalityCounts::default();
    for record in &records {
        cardinalities.record(&record.outcome);
    }
    stats.records_total = records.len() as u64;
    stats.cardinalities = cardinalities.clone();

    // 4. Assemble the manifest and digest it.
    let fingerprint = CandidateFingerprint::current();
    let mut manifest = CandidateManifest {
        candidate_manifest_version: CANDIDATE_MANIFEST_VERSION,
        candidate_schema_version: CANDIDATE_SCHEMA_VERSION,
        candidate_rule_abi_version: CANDIDATE_RULE_ABI_VERSION,
        snapshot_digest: snapshot_manifest.snapshot_digest.clone(),
        snapshot_schema_version: snapshot_manifest.schema_version,
        snapshot_analyzer_fingerprint: snapshot_manifest.analyzer_fingerprint.clone(),
        link_digest: link_manifest.link_digest.clone(),
        link_fingerprint: link_manifest.link_fingerprint.clone(),
        link_rule_abi_version: link_manifest.link_rule_abi_version,
        candidate_fingerprint: fingerprint.digest.clone(),
        candidate_fingerprint_text: fingerprint.text.clone(),
        candidate_digest: String::new(),
        records: records.len() as u64,
        cardinalities,
        rules: candidate_rule_registry(),
    };
    manifest.refresh_candidate_digest(&records);

    // 5. Write into staging, verify it there, then publish.
    let serialize_started = Instant::now();
    let staging = artifact::prepare_staging(output)?;
    let result = artifact::write(&staging, &manifest, &records);
    phases.serialize_ms = elapsed_ms(serialize_started);
    if let Err(error) = result {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(error);
    }

    let verify_started = Instant::now();
    let verified = artifact::verify(&staging, snapshot_dir, links_dir);
    phases.verify_ms = elapsed_ms(verify_started);
    if let Err(error) = verified {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(error);
    }

    artifact::publish(&staging, output)?;

    stats.candidate_bytes = artifact::artifact_size(output);
    stats.duration_ms = elapsed_ms(started);
    stats.phases = phases;
    Ok(CandidateBuildOutcome { manifest, stats })
}

fn elapsed_ms(started: Instant) -> f64 {
    started.elapsed().as_secs_f64() * 1000.0
}
