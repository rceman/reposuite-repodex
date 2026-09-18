//! Derived cross-file link build.
//!
//! ```text
//! TASK 3A snapshot
//!     -> load manifest + normalized file facts
//!     -> read the metadata a required rule needs (repository-root go.mod)
//!     -> derive structural identities
//!     -> apply the bounded per-language link rules
//!     -> order, count, digest
//!     -> verify in staging, then publish atomically
//! ```
//!
//! The build never mutates the snapshot, never reparses source through
//! Tree-sitter, and never writes a second copy of the normalized facts.
//!
//! TASK 3B rebuilds the whole derived artifact from the whole snapshot. The
//! measured cost (see `docs/TASK3B_RESULTS.md`) does not justify incremental
//! relationship patching yet, and a full rebuild is much easier to keep
//! deterministic.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Instant;

use crate::model::FileAnalysis;
use crate::repository::artifact as snapshot_artifact;
use crate::repository::manifest::RepositoryManifest;

use super::artifact::{self, LinkError};
use super::metadata::{self, GoModuleState};
use super::model::{
    rule_registry, LinkFingerprint, LinkManifest, LinkRecord, MetadataDependency, OutcomeCounts,
    LINK_MANIFEST_VERSION, LINK_RULE_ABI_VERSION, LINK_SCHEMA_VERSION,
};
use super::structure::Structure;
use super::{rules_go, rules_php, rules_python, rules_rust};

/// Wall-clock cost of each link-build phase, in milliseconds.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LinkPhaseTimings {
    pub snapshot_load_ms: f64,
    pub metadata_ms: f64,
    pub structure_ms: f64,
    pub derive_ms: f64,
    pub serialize_ms: f64,
    pub verify_ms: f64,
}

/// Work actually performed by one link build.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LinkBuildStats {
    pub snapshot_files: u64,
    pub links_total: u64,
    pub outcomes: OutcomeCounts,
    /// Link counts by kind, in canonical key order.
    pub kinds: BTreeMap<String, u64>,
    pub structural_entities: u64,
    pub metadata_dependencies: u64,
    pub snapshot_bytes: u64,
    pub link_bytes: u64,
    pub duration_ms: f64,
    pub phases: LinkPhaseTimings,
}

impl LinkBuildStats {
    /// Link artifact bytes as a fraction of the snapshot artifact bytes.
    pub fn size_ratio(&self) -> f64 {
        if self.snapshot_bytes == 0 {
            0.0
        } else {
            self.link_bytes as f64 / self.snapshot_bytes as f64
        }
    }
}

/// Result of a successful link build.
#[derive(Debug, Clone)]
pub struct LinkBuildOutcome {
    pub manifest: LinkManifest,
    pub stats: LinkBuildStats,
}

/// Build the derived cross-file link artifact for `snapshot_dir` into `output`.
///
/// `repository` is the checkout root, used only to read the metadata a required
/// rule needs. Without it, Go import paths cannot be shown to be local and are
/// classified as external, which is recorded rather than guessed.
pub fn build_links(
    snapshot_dir: &Path,
    repository: Option<&Path>,
    output: &Path,
) -> Result<LinkBuildOutcome, LinkError> {
    let started = Instant::now();
    let mut stats = LinkBuildStats::default();
    let mut phases = LinkPhaseTimings::default();

    // 1. Load the snapshot.
    let load_started = Instant::now();
    let snapshot_manifest: RepositoryManifest = snapshot_artifact::read_manifest(snapshot_dir)
        .map_err(|error| LinkError::InvalidManifest {
            reason: format!("the snapshot could not be read: {error}"),
        })?;
    let mut analyses: Vec<FileAnalysis> = Vec::with_capacity(snapshot_manifest.files.len());
    for file in &snapshot_manifest.files {
        analyses.push(
            snapshot_artifact::read_file_analysis(snapshot_dir, file).map_err(|error| {
                LinkError::InvalidManifest {
                    reason: format!("snapshot artifact `{}`: {error}", file.relative_path),
                }
            })?,
        );
    }
    stats.snapshot_files = analyses.len() as u64;
    stats.snapshot_bytes = snapshot_artifact::artifact_size(snapshot_dir);
    phases.snapshot_load_ms = elapsed_ms(load_started);

    // 2. Read the metadata a required rule needs.
    let metadata_started = Instant::now();
    let go_module = match repository {
        Some(root) => metadata::read_go_module(root),
        None => GoModuleState::Absent,
    };
    phases.metadata_ms = elapsed_ms(metadata_started);

    // 3. Derive structural identities.
    let structure_started = Instant::now();
    let structure = Structure::build(&analyses);
    phases.structure_ms = elapsed_ms(structure_started);

    // 4. Apply the bounded per-language rules.
    let derive_started = Instant::now();
    let mut links: Vec<LinkRecord> = Vec::new();
    links.extend(rules_rust::links(&structure, &analyses));
    links.extend(rules_go::links(&structure, &analyses, &go_module));
    links.extend(rules_python::links(&structure, &analyses));
    links.extend(rules_php::links(&structure, &analyses));
    artifact::order_links(&mut links)?;
    let mut entities = structure.entities.clone();
    artifact::order_entities(&mut entities);
    phases.derive_ms = elapsed_ms(derive_started);

    // 5. Count and collect metadata dependencies.
    let mut outcomes = OutcomeCounts::default();
    let mut kinds: BTreeMap<String, u64> = BTreeMap::new();
    let mut metadata_dependencies: BTreeMap<String, MetadataDependency> = BTreeMap::new();
    for link in &links {
        outcomes.record(&link.outcome);
        *kinds.entry(link.kind.clone()).or_default() += 1;
        for dependency in &link.provenance.metadata {
            metadata_dependencies.insert(dependency_key(dependency), dependency.clone());
        }
    }
    // The Go rules depend on the presence or absence of a `go.mod` whenever the
    // snapshot contains a Go file, even if no Go import was classified.
    if analyses
        .iter()
        .any(|analysis| analysis.file.language == crate::model::LanguageId::Go)
    {
        let dependency = go_module.dependency(super::model::rule::GO_IMPORT_LOCAL_MODULE);
        metadata_dependencies.insert(dependency_key(&dependency), dependency);
    }
    let metadata_dependencies: Vec<MetadataDependency> =
        metadata_dependencies.into_values().collect();

    stats.links_total = links.len() as u64;
    stats.outcomes = outcomes.clone();
    stats.kinds = kinds.clone();
    stats.structural_entities = entities.len() as u64;
    stats.metadata_dependencies = metadata_dependencies.len() as u64;

    // 6. Assemble the manifest and digest it.
    let fingerprint = LinkFingerprint::current();
    let mut manifest = LinkManifest {
        link_manifest_version: LINK_MANIFEST_VERSION,
        link_schema_version: LINK_SCHEMA_VERSION,
        link_rule_abi_version: LINK_RULE_ABI_VERSION,
        snapshot_digest: snapshot_manifest.snapshot_digest.clone(),
        snapshot_schema_version: snapshot_manifest.schema_version,
        snapshot_analyzer_fingerprint: snapshot_manifest.analyzer_fingerprint.clone(),
        link_fingerprint: fingerprint.digest.clone(),
        link_fingerprint_text: fingerprint.text.clone(),
        link_digest: String::new(),
        links: links.len() as u64,
        structural_entities: entities.len() as u64,
        outcomes,
        kinds,
        rules: rule_registry(),
        metadata: metadata_dependencies,
    };
    manifest.refresh_link_digest(&links, &entities);

    // 7. Write into staging, verify it there, then publish.
    let serialize_started = Instant::now();
    let staging = artifact::prepare_staging(output)?;
    let result = (|| -> Result<(), LinkError> {
        artifact::write(&staging, &manifest, &links, &entities)?;
        Ok(())
    })();
    phases.serialize_ms = elapsed_ms(serialize_started);
    if let Err(error) = result {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(error);
    }

    let verify_started = Instant::now();
    let verified = artifact::verify(&staging, snapshot_dir, repository);
    phases.verify_ms = elapsed_ms(verify_started);
    if let Err(error) = verified {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(error);
    }

    artifact::publish(&staging, output)?;

    stats.link_bytes = artifact::artifact_size(output);
    stats.duration_ms = elapsed_ms(started);
    stats.phases = phases;
    Ok(LinkBuildOutcome { manifest, stats })
}

fn dependency_key(dependency: &MetadataDependency) -> String {
    format!(
        "{}\u{1}{}\u{1}{}\u{1}{}\u{1}{}",
        dependency.relative_path,
        dependency.rule_id,
        dependency.field,
        dependency.value,
        dependency.present
    )
}

fn elapsed_ms(started: Instant) -> f64 {
    started.elapsed().as_secs_f64() * 1000.0
}
