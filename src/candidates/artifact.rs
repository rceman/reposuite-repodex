//! On-disk call-candidate artifact: write, read, verify and publish.
//!
//! Layout (EXPERIMENTAL, no compatibility promise):
//!
//! ```text
//! <candidates-dir>/
//!   manifest.json            canonical manifest (pretty JSON)
//!   call_candidates.jsonl    one candidate record per line
//! ```
//!
//! The artifact does **not** contain a second copy of the normalized file
//! facts, of the TASK 3B links or of source code. Every record references the
//! TASK 3A snapshot through a snapshot-local locator.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use crate::model::{DeclarationKind, FileAnalysis};
use crate::repository::digest;
use crate::repository::manifest::RepositoryManifest;

use super::model::{
    CallCandidateRecord, CandidateManifest, CandidateOutcome, CANDIDATE_MANIFEST_VERSION,
};

/// Manifest file name inside a candidate artifact directory.
pub const MANIFEST_FILE: &str = "manifest.json";
/// Candidate record file name inside a candidate artifact directory.
pub const RECORDS_FILE: &str = "call_candidates.jsonl";

const STAGING_SUFFIX: &str = ".building";
const BACKUP_SUFFIX: &str = ".previous";

/// A candidate artifact failure.
#[derive(Debug)]
pub enum CandidateError {
    Io {
        path: PathBuf,
        message: String,
    },
    Json {
        path: PathBuf,
        message: String,
    },
    MissingManifest {
        dir: PathBuf,
    },
    UnsupportedManifestVersion {
        found: u32,
        expected: u32,
    },
    InvalidManifest {
        reason: String,
    },
    /// The artifact records a different snapshot dependency than the caller
    /// supplied.
    SnapshotMismatch {
        expected: String,
        found: String,
    },
    /// The artifact records a different TASK 3B link dependency than the caller
    /// supplied.
    LinkMismatch {
        expected: String,
        found: String,
    },
    /// A referenced source call locator does not exist in the snapshot.
    MissingLocator {
        locator: String,
    },
    /// A candidate declaration locator does not exist in the snapshot.
    MissingCandidate {
        locator: String,
    },
    /// A candidate declaration exists but is not an allowed kind.
    ForbiddenCandidateKind {
        locator: String,
        kind: String,
    },
    /// The same record id appears twice.
    DuplicateRecordId {
        record_id: String,
    },
    /// The record list is not in canonical order.
    UnsortedRecords,
    /// A record's cardinality does not match its candidate count.
    CardinalityMismatch {
        record_id: String,
    },
    /// A candidate list is not sorted or contains a duplicate.
    UnsortedCandidates {
        record_id: String,
    },
    /// A record references a rule the manifest does not document.
    UndocumentedRule {
        rule_id: String,
    },
    /// The artifact was derived by a different candidate-rule version than the
    /// one verifying it, so it is not analysis-compatible with this build.
    RuleFingerprintMismatch {
        expected: String,
        found: String,
    },
    /// The recomputed candidate digest does not match the manifest.
    CandidateDigestMismatch {
        expected: String,
        found: String,
    },
}

impl fmt::Display for CandidateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CandidateError::Io { path, message } => {
                write!(formatter, "i/o error at {}: {message}", path.display())
            }
            CandidateError::Json { path, message } => {
                write!(formatter, "invalid JSON at {}: {message}", path.display())
            }
            CandidateError::MissingManifest { dir } => write!(
                formatter,
                "{} is not a candidate artifact: no {MANIFEST_FILE}",
                dir.display()
            ),
            CandidateError::UnsupportedManifestVersion { found, expected } => write!(
                formatter,
                "unsupported candidate manifest version {found} (this build understands {expected})"
            ),
            CandidateError::InvalidManifest { reason } => {
                write!(formatter, "invalid candidate manifest: {reason}")
            }
            CandidateError::SnapshotMismatch { expected, found } => write!(
                formatter,
                "candidate artifact depends on snapshot {found}, but the supplied snapshot is {expected}"
            ),
            CandidateError::LinkMismatch { expected, found } => write!(
                formatter,
                "candidate artifact depends on link artifact {found}, but the supplied link artifact is {expected}"
            ),
            CandidateError::MissingLocator { locator } => write!(
                formatter,
                "candidate record references a call fact that is not in the snapshot: {locator}"
            ),
            CandidateError::MissingCandidate { locator } => write!(
                formatter,
                "candidate record references a declaration that is not in the snapshot: {locator}"
            ),
            CandidateError::ForbiddenCandidateKind { locator, kind } => write!(
                formatter,
                "candidate {locator} has kind `{kind}`, which is not an allowed candidate kind"
            ),
            CandidateError::DuplicateRecordId { record_id } => {
                write!(formatter, "candidate record id `{record_id}` appears more than once")
            }
            CandidateError::UnsortedRecords => {
                write!(formatter, "candidate record list is not in canonical order")
            }
            CandidateError::CardinalityMismatch { record_id } => write!(
                formatter,
                "candidate record `{record_id}` has a cardinality that does not match its candidate count"
            ),
            CandidateError::UnsortedCandidates { record_id } => write!(
                formatter,
                "candidate record `{record_id}` has an unsorted or duplicated candidate list"
            ),
            CandidateError::UndocumentedRule { rule_id } => write!(
                formatter,
                "candidate record references rule `{rule_id}` which the manifest does not document"
            ),
            CandidateError::RuleFingerprintMismatch { expected, found } => write!(
                formatter,
                "candidate artifact was derived by rule fingerprint {found}, but this build expects {expected}"
            ),
            CandidateError::CandidateDigestMismatch { expected, found } => write!(
                formatter,
                "candidate digest mismatch: manifest says {expected}, content is {found}"
            ),
        }
    }
}

impl std::error::Error for CandidateError {}

/// Result of verifying a candidate artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateVerificationReport {
    pub candidate_manifest_version: u32,
    pub candidate_schema_version: u32,
    pub candidate_rule_abi_version: u32,
    pub snapshot_digest: String,
    pub link_digest: String,
    pub candidate_digest: String,
    pub records: u64,
    pub cardinalities: super::model::CardinalityCounts,
    /// Total on-disk size of the artifact, in bytes.
    pub artifact_bytes: u64,
}

/// Canonical sort key for one candidate record.
fn record_sort_key(record: &CallCandidateRecord) -> (String, u32, String, String) {
    (
        record.source.relative_path.clone(),
        record.source.fact_id,
        record.rule_id.clone(),
        record.written.clone(),
    )
}

/// Put candidate records into canonical order and reject duplicate ids.
pub fn order_records(records: &mut [CallCandidateRecord]) -> Result<(), CandidateError> {
    records.sort_by_key(record_sort_key);
    for pair in records.windows(2) {
        if pair[0].record_id == pair[1].record_id {
            return Err(CandidateError::DuplicateRecordId {
                record_id: pair[0].record_id.clone(),
            });
        }
    }
    Ok(())
}

/// Directory a build writes into before publishing.
pub fn staging_dir(output: &Path) -> PathBuf {
    let mut name = output
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "candidates".to_string());
    name.push_str(STAGING_SUFFIX);
    output.with_file_name(name)
}

/// Sibling directory a publish moves an existing output to before replacing it.
pub fn backup_dir(output: &Path) -> PathBuf {
    let mut name = output
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "candidates".to_string());
    name.push_str(BACKUP_SUFFIX);
    output.with_file_name(name)
}

/// Create a clean staging directory next to `output`.
pub fn prepare_staging(output: &Path) -> Result<PathBuf, CandidateError> {
    let staging = staging_dir(output);
    if staging.exists() {
        std::fs::remove_dir_all(&staging).map_err(|error| CandidateError::Io {
            path: staging.clone(),
            message: error.to_string(),
        })?;
    }
    std::fs::create_dir_all(&staging).map_err(|error| CandidateError::Io {
        path: staging.clone(),
        message: error.to_string(),
    })?;
    Ok(staging)
}

/// Publish a fully built staging directory as `output`.
pub fn publish(staging: &Path, output: &Path) -> Result<(), CandidateError> {
    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|error| CandidateError::Io {
                path: parent.to_path_buf(),
                message: error.to_string(),
            })?;
        }
    }
    if output.exists() {
        let backup = backup_dir(output);
        if backup.exists() {
            std::fs::remove_dir_all(&backup).map_err(|error| CandidateError::Io {
                path: backup.clone(),
                message: error.to_string(),
            })?;
        }
        std::fs::rename(output, &backup).map_err(|error| CandidateError::Io {
            path: output.to_path_buf(),
            message: error.to_string(),
        })?;
        match std::fs::rename(staging, output) {
            Ok(()) => {
                let _ = std::fs::remove_dir_all(&backup);
                Ok(())
            }
            Err(error) => {
                let _ = std::fs::rename(&backup, output);
                Err(CandidateError::Io {
                    path: output.to_path_buf(),
                    message: error.to_string(),
                })
            }
        }
    } else {
        std::fs::rename(staging, output).map_err(|error| CandidateError::Io {
            path: output.to_path_buf(),
            message: error.to_string(),
        })
    }
}

/// Write a manifest and its candidate records.
pub fn write(
    staging: &Path,
    manifest: &CandidateManifest,
    records: &[CallCandidateRecord],
) -> Result<(), CandidateError> {
    let manifest_path = staging.join(MANIFEST_FILE);
    let bytes = serde_json::to_vec_pretty(manifest).map_err(|error| CandidateError::Json {
        path: manifest_path.clone(),
        message: error.to_string(),
    })?;
    std::fs::write(&manifest_path, bytes).map_err(|error| CandidateError::Io {
        path: manifest_path,
        message: error.to_string(),
    })?;

    write_jsonl(staging, RECORDS_FILE, records)?;
    Ok(())
}

fn write_jsonl<T: serde::Serialize>(
    staging: &Path,
    name: &str,
    values: &[T],
) -> Result<(), CandidateError> {
    let path = staging.join(name);
    let mut buffer = Vec::new();
    for value in values {
        serde_json::to_writer(&mut buffer, value).map_err(|error| CandidateError::Json {
            path: path.clone(),
            message: error.to_string(),
        })?;
        buffer.push(b'\n');
    }
    std::fs::write(&path, buffer).map_err(|error| CandidateError::Io {
        path,
        message: error.to_string(),
    })
}

/// Read and structurally validate a candidate manifest.
pub fn read_manifest(dir: &Path) -> Result<CandidateManifest, CandidateError> {
    let path = dir.join(MANIFEST_FILE);
    if !path.is_file() {
        return Err(CandidateError::MissingManifest {
            dir: dir.to_path_buf(),
        });
    }
    let bytes = std::fs::read(&path).map_err(|error| CandidateError::Io {
        path: path.clone(),
        message: error.to_string(),
    })?;
    let manifest: CandidateManifest =
        serde_json::from_slice(&bytes).map_err(|error| CandidateError::Json {
            path: path.clone(),
            message: error.to_string(),
        })?;
    validate_manifest(&manifest)?;
    Ok(manifest)
}

/// Structural validation of a candidate manifest that does not read records.
pub fn validate_manifest(manifest: &CandidateManifest) -> Result<(), CandidateError> {
    if manifest.candidate_manifest_version != CANDIDATE_MANIFEST_VERSION {
        return Err(CandidateError::UnsupportedManifestVersion {
            found: manifest.candidate_manifest_version,
            expected: CANDIDATE_MANIFEST_VERSION,
        });
    }
    for (field, value) in [
        ("snapshot_digest", &manifest.snapshot_digest),
        (
            "snapshot_analyzer_fingerprint",
            &manifest.snapshot_analyzer_fingerprint,
        ),
        ("link_digest", &manifest.link_digest),
        ("link_fingerprint", &manifest.link_fingerprint),
        ("candidate_fingerprint", &manifest.candidate_fingerprint),
        ("candidate_digest", &manifest.candidate_digest),
    ] {
        if !digest::is_digest(value) {
            return Err(CandidateError::InvalidManifest {
                reason: format!("{field} `{value}` is not a sha256 digest"),
            });
        }
    }
    for rule in &manifest.rules {
        if rule.rule_id.is_empty() {
            return Err(CandidateError::InvalidManifest {
                reason: "a candidate-rule registry entry has an empty rule_id".to_string(),
            });
        }
    }
    Ok(())
}

/// Read every candidate record from a candidate artifact directory.
pub fn read_records(dir: &Path) -> Result<Vec<CallCandidateRecord>, CandidateError> {
    let path = dir.join(RECORDS_FILE);
    let bytes = std::fs::read(&path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            CandidateError::InvalidManifest {
                reason: format!("{RECORDS_FILE} is missing from the artifact"),
            }
        } else {
            CandidateError::Io {
                path: path.clone(),
                message: error.to_string(),
            }
        }
    })?;
    let text = String::from_utf8(bytes).map_err(|_| CandidateError::Json {
        path: path.clone(),
        message: "not valid UTF-8".to_string(),
    })?;
    let mut values = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let value = serde_json::from_str(line).map_err(|error| CandidateError::Json {
            path: path.clone(),
            message: format!("line {}: {error}", index + 1),
        })?;
        values.push(value);
    }
    Ok(values)
}

/// Fully verify a candidate artifact against the snapshot and link artifact it
/// claims to depend on.
///
/// A successful verification means the artifact is **internally consistent with
/// the recorded snapshot and link artifact**. It does **not** prove any call
/// resolves to a candidate at runtime.
pub fn verify(
    dir: &Path,
    snapshot_dir: &Path,
    links_dir: &Path,
) -> Result<CandidateVerificationReport, CandidateError> {
    let manifest = read_manifest(dir)?;

    // 0. The artifact must have been derived by this build's candidate rule. A
    //    stale artifact from an earlier rule version carries a different
    //    fingerprint, so it is not analysis-compatible even if its digests are
    //    internally consistent.
    let current_fingerprint = super::model::CandidateFingerprint::current().digest;
    if manifest.candidate_fingerprint != current_fingerprint {
        return Err(CandidateError::RuleFingerprintMismatch {
            expected: current_fingerprint,
            found: manifest.candidate_fingerprint.clone(),
        });
    }

    // 1. The artifact must depend on exactly the supplied snapshot.
    let snapshot_manifest: RepositoryManifest =
        crate::repository::artifact::read_manifest(snapshot_dir).map_err(|error| {
            CandidateError::InvalidManifest {
                reason: format!("the supplied snapshot could not be read: {error}"),
            }
        })?;
    if snapshot_manifest.snapshot_digest != manifest.snapshot_digest {
        return Err(CandidateError::SnapshotMismatch {
            expected: snapshot_manifest.snapshot_digest,
            found: manifest.snapshot_digest,
        });
    }

    // 2. The artifact must depend on exactly the supplied TASK 3B link artifact.
    let link_manifest = crate::links::artifact::read_manifest(links_dir).map_err(|error| {
        CandidateError::InvalidManifest {
            reason: format!("the supplied link artifact could not be read: {error}"),
        }
    })?;
    if link_manifest.link_digest != manifest.link_digest {
        return Err(CandidateError::LinkMismatch {
            expected: link_manifest.link_digest,
            found: manifest.link_digest,
        });
    }
    // The link artifact's own snapshot dependency must agree too: a candidate
    // artifact built on a different upstream pipeline must not verify.
    if link_manifest.snapshot_digest != manifest.snapshot_digest {
        return Err(CandidateError::SnapshotMismatch {
            expected: manifest.snapshot_digest.clone(),
            found: link_manifest.snapshot_digest,
        });
    }

    // 3. Canonical ordering and unique record ids.
    let records = read_records(dir)?;
    let mut ordered = records.clone();
    order_records(&mut ordered)?;
    if ordered != records {
        return Err(CandidateError::UnsortedRecords);
    }

    // 4. Every referenced rule is documented.
    let documented: BTreeMap<&str, ()> = manifest
        .rules
        .iter()
        .map(|rule| (rule.rule_id.as_str(), ()))
        .collect();
    for record in &records {
        if !documented.contains_key(record.rule_id.as_str()) {
            return Err(CandidateError::UndocumentedRule {
                rule_id: record.rule_id.clone(),
            });
        }
    }

    // 5. Every locator exists in the snapshot, every candidate is an allowed
    //    kind, every candidate list is sorted and duplicate-free, and the
    //    recorded cardinality matches the candidate count.
    let mut analyses: BTreeMap<String, FileAnalysis> = BTreeMap::new();
    for file in &snapshot_manifest.files {
        analyses.insert(
            file.relative_path.clone(),
            crate::repository::artifact::read_file_analysis(snapshot_dir, file).map_err(
                |error| CandidateError::InvalidManifest {
                    reason: format!("snapshot artifact `{}`: {error}", file.relative_path),
                },
            )?,
        );
    }
    for record in &records {
        check_call_locator(&analyses, &record.source.key())?;
        check_cardinality(record)?;
        check_candidate_order(record)?;
        for candidate in record.outcome.candidates() {
            check_candidate_declaration(&analyses, record, candidate)?;
        }
    }

    // 6. The candidate digest covers the manifest dependency block and the
    //    records.
    let found = manifest.compute_candidate_digest(&records);
    if found != manifest.candidate_digest {
        return Err(CandidateError::CandidateDigestMismatch {
            expected: manifest.candidate_digest.clone(),
            found,
        });
    }

    let mut cardinalities = super::model::CardinalityCounts::default();
    for record in &records {
        cardinalities.record(&record.outcome);
    }

    Ok(CandidateVerificationReport {
        candidate_manifest_version: manifest.candidate_manifest_version,
        candidate_schema_version: manifest.candidate_schema_version,
        candidate_rule_abi_version: manifest.candidate_rule_abi_version,
        snapshot_digest: manifest.snapshot_digest.clone(),
        link_digest: manifest.link_digest.clone(),
        candidate_digest: manifest.candidate_digest.clone(),
        records: records.len() as u64,
        cardinalities,
        artifact_bytes: artifact_size(dir),
    })
}

/// The recorded cardinality must agree with the actual candidate count.
fn check_cardinality(record: &CallCandidateRecord) -> Result<(), CandidateError> {
    let count = record.outcome.candidates().len();
    let ok = match &record.outcome {
        CandidateOutcome::NoCandidate { .. } | CandidateOutcome::OutOfScope { .. } => count == 0,
        CandidateOutcome::SingleCandidate { .. } => count == 1,
        CandidateOutcome::MultipleCandidates { .. } => count >= 2,
    };
    if !ok {
        return Err(CandidateError::CardinalityMismatch {
            record_id: record.record_id.clone(),
        });
    }
    Ok(())
}

/// The candidate list of a record must be in canonical order and duplicate-free.
fn check_candidate_order(record: &CallCandidateRecord) -> Result<(), CandidateError> {
    let candidates = record.outcome.candidates();
    let mut ordered = candidates.to_vec();
    super::model::sort_candidates(&mut ordered);
    if ordered.len() != candidates.len() || ordered != candidates.to_vec() {
        return Err(CandidateError::UnsortedCandidates {
            record_id: record.record_id.clone(),
        });
    }
    Ok(())
}

/// A source call locator must name a call fact in the snapshot.
fn check_call_locator(
    analyses: &BTreeMap<String, FileAnalysis>,
    key: &str,
) -> Result<(), CandidateError> {
    let Some((path, rest)) = key.split_once('#') else {
        return Err(CandidateError::MissingLocator {
            locator: key.to_string(),
        });
    };
    let Some(analysis) = analyses.get(path) else {
        return Err(CandidateError::MissingLocator {
            locator: key.to_string(),
        });
    };
    let Some((kind, id)) = rest.split_once(':') else {
        return Err(CandidateError::MissingLocator {
            locator: key.to_string(),
        });
    };
    if kind != "call" {
        return Err(CandidateError::MissingLocator {
            locator: key.to_string(),
        });
    }
    let id: u32 = id
        .split('+')
        .next()
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| CandidateError::MissingLocator {
            locator: key.to_string(),
        })?;
    if (id as usize) >= analysis.calls.len() {
        return Err(CandidateError::MissingLocator {
            locator: key.to_string(),
        });
    }
    Ok(())
}

/// A candidate declaration locator must name a `function` declaration in the
/// snapshot file the candidate claims to live in.
fn check_candidate_declaration(
    analyses: &BTreeMap<String, FileAnalysis>,
    record: &CallCandidateRecord,
    candidate: &super::model::CandidateTarget,
) -> Result<(), CandidateError> {
    let locator = format!(
        "{}#declaration:{}",
        candidate.relative_path, candidate.declaration_id
    );
    let Some(analysis) = analyses.get(&candidate.relative_path) else {
        return Err(CandidateError::MissingCandidate { locator });
    };
    let Some(declaration) = analysis.declarations.get(candidate.declaration_id as usize) else {
        return Err(CandidateError::MissingCandidate { locator });
    };
    if declaration.kind != DeclarationKind::Function {
        return Err(CandidateError::ForbiddenCandidateKind {
            locator,
            kind: declaration.kind.as_str().to_string(),
        });
    }
    let _ = record;
    Ok(())
}

/// Total on-disk size of a candidate artifact directory, in bytes.
pub fn artifact_size(dir: &Path) -> u64 {
    let mut total = 0u64;
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    for entry in entries.flatten() {
        if let Ok(metadata) = entry.metadata() {
            if metadata.is_file() {
                total += metadata.len();
            }
        }
    }
    total
}
