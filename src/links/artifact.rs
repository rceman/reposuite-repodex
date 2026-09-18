//! On-disk cross-file link artifact: write, read, verify and publish.
//!
//! Layout (EXPERIMENTAL, no compatibility promise):
//!
//! ```text
//! <links-dir>/
//!   manifest.json      canonical manifest (pretty JSON, human-inspectable)
//!   links.jsonl        one derived relationship per line
//!   entities.jsonl     one derived structural identity per line
//! ```
//!
//! The artifact deliberately does **not** contain a second copy of the
//! normalized file facts. Every relationship references the TASK 3A snapshot
//! through a snapshot-local locator, so the link artifact stays a small derived
//! index rather than a duplicate of a snapshot that is already several times the
//! size of the source it describes.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use crate::model::FileAnalysis;
use crate::repository::digest;
use crate::repository::manifest::RepositoryManifest;

use super::model::{
    FactKind, LinkManifest, LinkRecord, LinkTarget, StructuralEntity, LINK_MANIFEST_VERSION,
};

/// Manifest file name inside a link artifact directory.
pub const MANIFEST_FILE: &str = "manifest.json";
/// Relationship file name inside a link artifact directory.
pub const LINKS_FILE: &str = "links.jsonl";
/// Structural identity file name inside a link artifact directory.
pub const ENTITIES_FILE: &str = "entities.jsonl";

const STAGING_SUFFIX: &str = ".building";
const BACKUP_SUFFIX: &str = ".previous";

/// A link artifact failure.
#[derive(Debug)]
pub enum LinkError {
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
    /// A referenced source fact locator does not exist in the snapshot.
    MissingLocator {
        locator: String,
    },
    /// A referenced structural entity does not exist in the artifact.
    MissingEntity {
        entity_id: String,
    },
    /// The same relationship id appears twice.
    DuplicateLinkId {
        link_id: String,
    },
    /// The relationship list is not in canonical order.
    UnsortedLinks,
    /// A link references a rule that the manifest does not document.
    UndocumentedRule {
        rule_id: String,
    },
    /// The recomputed link digest does not match the manifest.
    LinkDigestMismatch {
        expected: String,
        found: String,
    },
    /// A metadata file the artifact depends on has different content.
    MetadataMismatch {
        relative_path: String,
        expected: String,
        found: String,
    },
    /// A metadata file the artifact depends on is missing.
    MetadataMissing {
        relative_path: String,
    },
}

impl fmt::Display for LinkError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LinkError::Io { path, message } => {
                write!(formatter, "i/o error at {}: {message}", path.display())
            }
            LinkError::Json { path, message } => {
                write!(formatter, "invalid JSON at {}: {message}", path.display())
            }
            LinkError::MissingManifest { dir } => write!(
                formatter,
                "{} is not a link artifact: no {MANIFEST_FILE}",
                dir.display()
            ),
            LinkError::UnsupportedManifestVersion { found, expected } => write!(
                formatter,
                "unsupported link manifest version {found} (this build understands {expected})"
            ),
            LinkError::InvalidManifest { reason } => {
                write!(formatter, "invalid link manifest: {reason}")
            }
            LinkError::SnapshotMismatch { expected, found } => write!(
                formatter,
                "link artifact depends on snapshot {found}, but the supplied snapshot is {expected}"
            ),
            LinkError::MissingLocator { locator } => {
                write!(formatter, "link references a source fact that is not in the snapshot: {locator}")
            }
            LinkError::MissingEntity { entity_id } => {
                write!(formatter, "link references an unknown structural entity: {entity_id}")
            }
            LinkError::DuplicateLinkId { link_id } => {
                write!(formatter, "link id `{link_id}` appears more than once")
            }
            LinkError::UnsortedLinks => {
                write!(formatter, "link list is not in canonical order")
            }
            LinkError::UndocumentedRule { rule_id } => {
                write!(formatter, "link references rule `{rule_id}` which the manifest does not document")
            }
            LinkError::LinkDigestMismatch { expected, found } => write!(
                formatter,
                "link digest mismatch: manifest says {expected}, content is {found}"
            ),
            LinkError::MetadataMismatch {
                relative_path,
                expected,
                found,
            } => write!(
                formatter,
                "metadata `{relative_path}` has changed: artifact recorded {expected}, repository has {found}"
            ),
            LinkError::MetadataMissing { relative_path } => {
                write!(formatter, "metadata `{relative_path}` is recorded as a dependency but is absent")
            }
        }
    }
}

impl std::error::Error for LinkError {}

/// Result of verifying a link artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkVerificationReport {
    pub link_manifest_version: u32,
    pub link_schema_version: u32,
    pub link_rule_abi_version: u32,
    pub snapshot_digest: String,
    pub link_digest: String,
    pub links: u64,
    pub structural_entities: u64,
    pub outcomes: super::model::OutcomeCounts,
    /// Total on-disk size of the artifact, in bytes.
    pub artifact_bytes: u64,
    /// True when metadata dependencies were re-checked against a repository.
    pub metadata_checked: bool,
    pub metadata_dependencies: u64,
}

/// Canonical sort key for one relationship.
fn link_sort_key(link: &LinkRecord) -> (String, FactKind, u32, u32, String, String) {
    (
        link.source.relative_path.clone(),
        link.source.fact_kind,
        link.source.fact_id,
        link.source.item_index.unwrap_or(u32::MAX),
        link.rule_id.clone(),
        link.written.clone(),
    )
}

/// Put relationships into canonical order and reject duplicate ids.
pub fn order_links(links: &mut [LinkRecord]) -> Result<(), LinkError> {
    links.sort_by_key(link_sort_key);
    for pair in links.windows(2) {
        if pair[0].link_id == pair[1].link_id {
            return Err(LinkError::DuplicateLinkId {
                link_id: pair[0].link_id.clone(),
            });
        }
    }
    Ok(())
}

/// Put structural entities into canonical order.
pub fn order_entities(entities: &mut [StructuralEntity]) {
    entities.sort_by(|left, right| {
        (&left.structural_kind, &left.key).cmp(&(&right.structural_kind, &right.key))
    });
}

/// Directory a build writes into before publishing.
pub fn staging_dir(output: &Path) -> PathBuf {
    let mut name = output
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "links".to_string());
    name.push_str(STAGING_SUFFIX);
    output.with_file_name(name)
}

/// Sibling directory a publish moves an existing output to before replacing it.
pub fn backup_dir(output: &Path) -> PathBuf {
    let mut name = output
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "links".to_string());
    name.push_str(BACKUP_SUFFIX);
    output.with_file_name(name)
}

/// Create a clean staging directory next to `output`.
pub fn prepare_staging(output: &Path) -> Result<PathBuf, LinkError> {
    let staging = staging_dir(output);
    if staging.exists() {
        std::fs::remove_dir_all(&staging).map_err(|error| LinkError::Io {
            path: staging.clone(),
            message: error.to_string(),
        })?;
    }
    std::fs::create_dir_all(&staging).map_err(|error| LinkError::Io {
        path: staging.clone(),
        message: error.to_string(),
    })?;
    Ok(staging)
}

/// Publish a fully built staging directory as `output`.
///
/// The same never-half-written guarantee as the snapshot artifact: an existing
/// `output` is moved aside first, so at every instant a valid artifact exists
/// either at `output` or at the backup path.
pub fn publish(staging: &Path, output: &Path) -> Result<(), LinkError> {
    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|error| LinkError::Io {
                path: parent.to_path_buf(),
                message: error.to_string(),
            })?;
        }
    }
    if output.exists() {
        let backup = backup_dir(output);
        if backup.exists() {
            std::fs::remove_dir_all(&backup).map_err(|error| LinkError::Io {
                path: backup.clone(),
                message: error.to_string(),
            })?;
        }
        std::fs::rename(output, &backup).map_err(|error| LinkError::Io {
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
                Err(LinkError::Io {
                    path: output.to_path_buf(),
                    message: error.to_string(),
                })
            }
        }
    } else {
        std::fs::rename(staging, output).map_err(|error| LinkError::Io {
            path: output.to_path_buf(),
            message: error.to_string(),
        })
    }
}

/// Write a manifest, its relationships and its structural entities.
pub fn write(
    staging: &Path,
    manifest: &LinkManifest,
    links: &[LinkRecord],
    entities: &[StructuralEntity],
) -> Result<(), LinkError> {
    let manifest_path = staging.join(MANIFEST_FILE);
    let bytes = serde_json::to_vec_pretty(manifest).map_err(|error| LinkError::Json {
        path: manifest_path.clone(),
        message: error.to_string(),
    })?;
    std::fs::write(&manifest_path, bytes).map_err(|error| LinkError::Io {
        path: manifest_path,
        message: error.to_string(),
    })?;

    write_jsonl(staging, LINKS_FILE, links)?;
    write_jsonl(staging, ENTITIES_FILE, entities)?;
    Ok(())
}

fn write_jsonl<T: serde::Serialize>(
    staging: &Path,
    name: &str,
    values: &[T],
) -> Result<(), LinkError> {
    let path = staging.join(name);
    let mut buffer = Vec::new();
    for value in values {
        serde_json::to_writer(&mut buffer, value).map_err(|error| LinkError::Json {
            path: path.clone(),
            message: error.to_string(),
        })?;
        buffer.push(b'\n');
    }
    std::fs::write(&path, buffer).map_err(|error| LinkError::Io {
        path,
        message: error.to_string(),
    })
}

/// Read and structurally validate a link manifest.
pub fn read_manifest(dir: &Path) -> Result<LinkManifest, LinkError> {
    let path = dir.join(MANIFEST_FILE);
    if !path.is_file() {
        return Err(LinkError::MissingManifest {
            dir: dir.to_path_buf(),
        });
    }
    let bytes = std::fs::read(&path).map_err(|error| LinkError::Io {
        path: path.clone(),
        message: error.to_string(),
    })?;
    let manifest: LinkManifest =
        serde_json::from_slice(&bytes).map_err(|error| LinkError::Json {
            path: path.clone(),
            message: error.to_string(),
        })?;
    validate_manifest(&manifest)?;
    Ok(manifest)
}

/// Structural validation of a link manifest that does not read the artifacts.
pub fn validate_manifest(manifest: &LinkManifest) -> Result<(), LinkError> {
    if manifest.link_manifest_version != LINK_MANIFEST_VERSION {
        return Err(LinkError::UnsupportedManifestVersion {
            found: manifest.link_manifest_version,
            expected: LINK_MANIFEST_VERSION,
        });
    }
    for (field, value) in [
        ("snapshot_digest", &manifest.snapshot_digest),
        (
            "snapshot_analyzer_fingerprint",
            &manifest.snapshot_analyzer_fingerprint,
        ),
        ("link_fingerprint", &manifest.link_fingerprint),
        ("link_digest", &manifest.link_digest),
    ] {
        if !digest::is_digest(value) {
            return Err(LinkError::InvalidManifest {
                reason: format!("{field} `{value}` is not a sha256 digest"),
            });
        }
    }
    for rule in &manifest.rules {
        if rule.rule_id.is_empty() {
            return Err(LinkError::InvalidManifest {
                reason: "a rule registry entry has an empty rule_id".to_string(),
            });
        }
    }
    for dependency in &manifest.metadata {
        if dependency.relative_path.is_empty() {
            return Err(LinkError::InvalidManifest {
                reason: "a metadata dependency has an empty relative_path".to_string(),
            });
        }
        if !digest::is_digest(&dependency.content_digest) {
            return Err(LinkError::InvalidManifest {
                reason: format!(
                    "metadata dependency `{}` has a malformed content_digest `{}`",
                    dependency.relative_path, dependency.content_digest
                ),
            });
        }
    }
    Ok(())
}

/// Read every relationship from a link artifact directory.
pub fn read_links(dir: &Path) -> Result<Vec<LinkRecord>, LinkError> {
    read_jsonl(dir, LINKS_FILE)
}

/// Read every structural entity from a link artifact directory.
pub fn read_entities(dir: &Path) -> Result<Vec<StructuralEntity>, LinkError> {
    read_jsonl(dir, ENTITIES_FILE)
}

fn read_jsonl<T: serde::de::DeserializeOwned>(dir: &Path, name: &str) -> Result<Vec<T>, LinkError> {
    let path = dir.join(name);
    let bytes = std::fs::read(&path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            LinkError::InvalidManifest {
                reason: format!("{name} is missing from the artifact"),
            }
        } else {
            LinkError::Io {
                path: path.clone(),
                message: error.to_string(),
            }
        }
    })?;
    let text = String::from_utf8(bytes).map_err(|_| LinkError::Json {
        path: path.clone(),
        message: "not valid UTF-8".to_string(),
    })?;
    let mut values = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let value = serde_json::from_str(line).map_err(|error| LinkError::Json {
            path: path.clone(),
            message: format!("line {}: {error}", index + 1),
        })?;
        values.push(value);
    }
    Ok(values)
}

/// Fully verify a link artifact against the snapshot it claims to depend on.
///
/// A successful verification means the artifact is **internally consistent with
/// the recorded snapshot and metadata**. It does **not** mean every relationship
/// is semantically correct at runtime.
///
/// When `repository` is `Some`, metadata dependency digests are re-read and
/// compared. Without it, metadata dependencies are reported as unverified.
pub fn verify(
    dir: &Path,
    snapshot_dir: &Path,
    repository: Option<&Path>,
) -> Result<LinkVerificationReport, LinkError> {
    let manifest = read_manifest(dir)?;

    // 1. The artifact must depend on exactly the supplied snapshot.
    let snapshot_manifest: RepositoryManifest =
        crate::repository::artifact::read_manifest(snapshot_dir).map_err(|error| {
            LinkError::InvalidManifest {
                reason: format!("the supplied snapshot could not be read: {error}"),
            }
        })?;
    if snapshot_manifest.snapshot_digest != manifest.snapshot_digest {
        return Err(LinkError::SnapshotMismatch {
            expected: snapshot_manifest.snapshot_digest,
            found: manifest.snapshot_digest,
        });
    }

    // 2. Canonical ordering and unique relationship ids.
    let mut links = read_links(dir)?;
    let mut ordered = links.clone();
    order_links(&mut ordered)?;
    if ordered != links {
        return Err(LinkError::UnsortedLinks);
    }
    let entities = read_entities(dir)?;
    let mut ordered_entities = entities.clone();
    order_entities(&mut ordered_entities);
    if ordered_entities != entities {
        return Err(LinkError::UnsortedLinks);
    }

    // 3. Every referenced rule is documented.
    let documented: BTreeMap<&str, ()> = manifest
        .rules
        .iter()
        .map(|rule| (rule.rule_id.as_str(), ()))
        .collect();
    for link in &links {
        if !documented.contains_key(link.rule_id.as_str()) {
            return Err(LinkError::UndocumentedRule {
                rule_id: link.rule_id.clone(),
            });
        }
    }

    // 4. Every locator exists in the snapshot.
    let entity_ids: BTreeMap<&str, ()> = entities
        .iter()
        .map(|entity| (entity.entity_id.as_str(), ()))
        .collect();
    let mut analyses: BTreeMap<String, FileAnalysis> = BTreeMap::new();
    for file in &snapshot_manifest.files {
        analyses.insert(
            file.relative_path.clone(),
            crate::repository::artifact::read_file_analysis(snapshot_dir, file).map_err(
                |error| LinkError::InvalidManifest {
                    reason: format!("snapshot artifact `{}`: {error}", file.relative_path),
                },
            )?,
        );
    }
    for link in &links {
        check_locator(&analyses, &link.source.key())?;
        for target in link.outcome.all_targets() {
            match target {
                LinkTarget::File { relative_path } => {
                    if !analyses.contains_key(relative_path) {
                        return Err(LinkError::MissingLocator {
                            locator: format!("file:{relative_path}"),
                        });
                    }
                }
                LinkTarget::Declaration {
                    relative_path,
                    declaration_id,
                    ..
                } => check_locator(
                    &analyses,
                    &format!("{relative_path}#declaration:{declaration_id}"),
                )?,
                LinkTarget::Structure { entity_id, .. } => {
                    if !entity_ids.contains_key(entity_id.as_str()) {
                        return Err(LinkError::MissingEntity {
                            entity_id: entity_id.clone(),
                        });
                    }
                }
            }
        }
    }
    for entity in &entities {
        for file in &entity.files {
            if !analyses.contains_key(file) {
                return Err(LinkError::MissingLocator {
                    locator: format!("file:{file}"),
                });
            }
        }
    }

    // 5. The link digest covers the manifest dependency block, the entities and
    //    the relationships.
    let found = manifest.compute_link_digest(&links, &entities);
    if found != manifest.link_digest {
        return Err(LinkError::LinkDigestMismatch {
            expected: manifest.link_digest.clone(),
            found,
        });
    }

    // 6. Metadata dependencies, when a repository root is available.
    let mut metadata_checked = false;
    if let Some(repository) = repository {
        for dependency in &manifest.metadata {
            let path = repository.join(&dependency.relative_path);
            if !dependency.present {
                // The rule depended on the file being absent.
                if path.exists() {
                    let found = std::fs::read(&path)
                        .map(|bytes| digest::content_digest(&bytes))
                        .unwrap_or_else(|_| "unreadable".to_string());
                    return Err(LinkError::MetadataMismatch {
                        relative_path: dependency.relative_path.clone(),
                        expected: "absent".to_string(),
                        found,
                    });
                }
                continue;
            }
            let bytes = std::fs::read(&path).map_err(|_| LinkError::MetadataMissing {
                relative_path: dependency.relative_path.clone(),
            })?;
            let found = digest::content_digest(&bytes);
            if found != dependency.content_digest {
                return Err(LinkError::MetadataMismatch {
                    relative_path: dependency.relative_path.clone(),
                    expected: dependency.content_digest.clone(),
                    found,
                });
            }
        }
        metadata_checked = true;
    }

    links.shrink_to_fit();
    let mut outcomes = super::model::OutcomeCounts::default();
    for link in &links {
        outcomes.record(&link.outcome);
    }

    Ok(LinkVerificationReport {
        link_manifest_version: manifest.link_manifest_version,
        link_schema_version: manifest.link_schema_version,
        link_rule_abi_version: manifest.link_rule_abi_version,
        snapshot_digest: manifest.snapshot_digest.clone(),
        link_digest: manifest.link_digest.clone(),
        links: links.len() as u64,
        structural_entities: entities.len() as u64,
        outcomes,
        artifact_bytes: artifact_size(dir),
        metadata_checked,
        metadata_dependencies: manifest.metadata.len() as u64,
    })
}

fn check_locator(analyses: &BTreeMap<String, FileAnalysis>, key: &str) -> Result<(), LinkError> {
    let Some((path, rest)) = key.split_once('#') else {
        return Err(LinkError::MissingLocator {
            locator: key.to_string(),
        });
    };
    let Some(analysis) = analyses.get(path) else {
        return Err(LinkError::MissingLocator {
            locator: key.to_string(),
        });
    };
    let Some((kind, id)) = rest.split_once(':') else {
        return Err(LinkError::MissingLocator {
            locator: key.to_string(),
        });
    };
    let id: u32 = id
        .split('+')
        .next()
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| LinkError::MissingLocator {
            locator: key.to_string(),
        })?;
    let exists = match kind {
        "declaration" => (id as usize) < analysis.declarations.len(),
        "import" => (id as usize) < analysis.imports.len(),
        "reference" => (id as usize) < analysis.references.len(),
        "call" => (id as usize) < analysis.calls.len(),
        _ => false,
    };
    if !exists {
        return Err(LinkError::MissingLocator {
            locator: key.to_string(),
        });
    }
    Ok(())
}

/// Total on-disk size of a link artifact directory, in bytes.
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
