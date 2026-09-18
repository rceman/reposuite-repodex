//! On-disk snapshot artifact: read, write, verify and publish.
//!
//! Layout (EXPERIMENTAL, no compatibility promise):
//!
//! ```text
//! <snapshot-dir>/
//!   manifest.json          canonical manifest (pretty JSON, human-inspectable)
//!   files/
//!     <object-key>.json    one normalized FileAnalysis per indexed file
//! ```
//!
//! There is no database and no binary layout. A snapshot is a directory of JSON
//! documents plus a manifest that lists them. Nothing here serializes a
//! Tree-sitter tree or a source buffer: a persisted artifact is exactly the
//! normalized facts.

use std::fmt;
use std::path::{Path, PathBuf};

use crate::model::FileAnalysis;
use crate::repository::digest;
use crate::repository::fingerprint::MANIFEST_VERSION;
use crate::repository::manifest::{IndexedFile, RepositoryManifest};

/// Manifest file name inside a snapshot directory.
pub const MANIFEST_FILE: &str = "manifest.json";
/// Sub-directory holding per-file artifacts.
pub const FILES_DIR: &str = "files";

/// Suffix used for the staging directory a build writes into before publishing.
const STAGING_SUFFIX: &str = ".building";
/// Suffix used for the backup of an existing output directory during publish.
const BACKUP_SUFFIX: &str = ".previous";

/// A repository snapshot artifact failure.
#[derive(Debug)]
pub enum SnapshotError {
    Io {
        path: PathBuf,
        message: String,
    },
    Json {
        path: PathBuf,
        message: String,
    },
    /// The directory has no manifest.
    MissingManifest {
        dir: PathBuf,
    },
    /// The manifest's format version is not one this build understands.
    UnsupportedManifestVersion {
        found: u32,
        expected: u32,
    },
    /// The manifest is structurally invalid.
    InvalidManifest {
        reason: String,
    },
    /// A file record references an artifact that is not present.
    MissingArtifact {
        object_key: String,
    },
    /// A file artifact's recomputed digest does not match the manifest.
    ArtifactDigestMismatch {
        object_key: String,
        expected: String,
        found: String,
    },
    /// The manifest lists the same path twice.
    DuplicatePath {
        relative_path: String,
    },
    /// The manifest's file list is not in canonical path order.
    UnsortedFiles,
    /// The previous snapshot was produced by a different analyzer.
    FingerprintMismatch {
        previous: String,
        current: String,
    },
    /// The previous snapshot was produced under a different configuration.
    ConfigMismatch {
        previous: String,
        current: String,
    },
    /// The previous snapshot cannot be used at all.
    PreviousSnapshotUnusable {
        reason: String,
    },
}

impl fmt::Display for SnapshotError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SnapshotError::Io { path, message } => {
                write!(formatter, "i/o error at {}: {message}", path.display())
            }
            SnapshotError::Json { path, message } => {
                write!(formatter, "invalid JSON at {}: {message}", path.display())
            }
            SnapshotError::MissingManifest { dir } => write!(
                formatter,
                "{} is not a snapshot: no {MANIFEST_FILE}",
                dir.display()
            ),
            SnapshotError::UnsupportedManifestVersion { found, expected } => write!(
                formatter,
                "unsupported manifest version {found} (this build understands {expected})"
            ),
            SnapshotError::InvalidManifest { reason } => {
                write!(formatter, "invalid snapshot manifest: {reason}")
            }
            SnapshotError::MissingArtifact { object_key } => write!(
                formatter,
                "manifest references artifact `{object_key}` which is not present"
            ),
            SnapshotError::ArtifactDigestMismatch {
                object_key,
                expected,
                found,
            } => write!(
                formatter,
                "artifact `{object_key}` digest mismatch: manifest says {expected}, content is {found}"
            ),
            SnapshotError::DuplicatePath { relative_path } => {
                write!(formatter, "manifest lists `{relative_path}` more than once")
            }
            SnapshotError::UnsortedFiles => {
                write!(formatter, "manifest file list is not in canonical path order")
            }
            SnapshotError::FingerprintMismatch { previous, current } => write!(
                formatter,
                "previous snapshot analyzer fingerprint {previous} does not match {current}"
            ),
            SnapshotError::ConfigMismatch { previous, current } => write!(
                formatter,
                "previous snapshot configuration does not match: {previous} vs {current}"
            ),
            SnapshotError::PreviousSnapshotUnusable { reason } => {
                write!(formatter, "previous snapshot cannot be used: {reason}")
            }
        }
    }
}

impl std::error::Error for SnapshotError {}

/// Result of verifying a snapshot artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerificationReport {
    pub manifest_version: u32,
    pub snapshot_digest: String,
    pub files: u64,
    pub checked_artifacts: u64,
    /// Total on-disk size of the artifact files, in bytes.
    pub artifact_bytes: u64,
}

/// Stable artifact key for one indexed file.
///
/// Derived from the relative path and the content digest, so it is
/// deterministic, collision-resistant and independent of the checkout root.
/// Because the key changes with the content, an edited file can never collide
/// with the artifact of its previous revision.
pub fn object_key(relative_path: &str, content_digest: &str) -> String {
    let mut seed = String::with_capacity(relative_path.len() + content_digest.len() + 32);
    seed.push_str("repodex-file-object-v1\n");
    seed.push_str(relative_path);
    seed.push('\n');
    seed.push_str(content_digest);
    let digest = digest::sha256_text(&seed);
    // Drop the algorithm prefix; the key is a file name, not a digest record.
    digest
        .strip_prefix("sha256:")
        .unwrap_or(&digest)
        .to_string()
}

/// Directory a build writes into before publishing.
pub fn staging_dir(output: &Path) -> PathBuf {
    let mut name = output
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "snapshot".to_string());
    name.push_str(STAGING_SUFFIX);
    output.with_file_name(name)
}

/// Create a clean staging directory next to `output`.
pub fn prepare_staging(output: &Path) -> Result<PathBuf, SnapshotError> {
    let staging = staging_dir(output);
    if staging.exists() {
        std::fs::remove_dir_all(&staging).map_err(|error| SnapshotError::Io {
            path: staging.clone(),
            message: error.to_string(),
        })?;
    }
    std::fs::create_dir_all(staging.join(FILES_DIR)).map_err(|error| SnapshotError::Io {
        path: staging.clone(),
        message: error.to_string(),
    })?;
    Ok(staging)
}

/// Write the manifest into a staging directory.
pub fn write_manifest(staging: &Path, manifest: &RepositoryManifest) -> Result<(), SnapshotError> {
    let path = staging.join(MANIFEST_FILE);
    let bytes = serde_json::to_vec_pretty(manifest).map_err(|error| SnapshotError::Json {
        path: path.clone(),
        message: error.to_string(),
    })?;
    std::fs::write(&path, bytes).map_err(|error| SnapshotError::Io {
        path,
        message: error.to_string(),
    })
}

/// Write one file artifact into a staging directory.
pub fn write_file_artifact(
    staging: &Path,
    object_key: &str,
    analysis: &FileAnalysis,
) -> Result<(), SnapshotError> {
    let path = staging.join(FILES_DIR).join(format!("{object_key}.json"));
    let bytes = serde_json::to_vec(analysis).map_err(|error| SnapshotError::Json {
        path: path.clone(),
        message: error.to_string(),
    })?;
    std::fs::write(&path, bytes).map_err(|error| SnapshotError::Io {
        path,
        message: error.to_string(),
    })
}

/// Copy a file artifact verbatim from a previous snapshot into a staging
/// directory.
///
/// Reuse copies bytes rather than re-serializing the loaded analysis. The
/// serialization is deterministic, so a copy is byte-identical to what a fresh
/// write would produce, and copying avoids a needless deserialize/serialize
/// round trip on the reuse path.
pub fn copy_file_artifact(
    previous_dir: &Path,
    staging: &Path,
    object_key: &str,
) -> Result<(), SnapshotError> {
    let from = previous_dir
        .join(FILES_DIR)
        .join(format!("{object_key}.json"));
    let to = staging.join(FILES_DIR).join(format!("{object_key}.json"));
    std::fs::copy(&from, &to).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            SnapshotError::MissingArtifact {
                object_key: object_key.to_string(),
            }
        } else {
            SnapshotError::Io {
                path: from,
                message: error.to_string(),
            }
        }
    })?;
    Ok(())
}

/// Publish a fully built staging directory as `output`.
///
/// An existing `output` is moved aside to a sibling backup before the staging
/// directory is renamed into place, so an interrupted publish never leaves a
/// half-written directory where a valid snapshot used to be: at every instant
/// the previous valid snapshot exists either at `output` or at the backup path.
pub fn publish(staging: &Path, output: &Path) -> Result<(), SnapshotError> {
    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|error| SnapshotError::Io {
                path: parent.to_path_buf(),
                message: error.to_string(),
            })?;
        }
    }
    if output.exists() {
        let backup = backup_dir(output);
        if backup.exists() {
            std::fs::remove_dir_all(&backup).map_err(|error| SnapshotError::Io {
                path: backup.clone(),
                message: error.to_string(),
            })?;
        }
        std::fs::rename(output, &backup).map_err(|error| SnapshotError::Io {
            path: output.to_path_buf(),
            message: error.to_string(),
        })?;
        match std::fs::rename(staging, output) {
            Ok(()) => {
                let _ = std::fs::remove_dir_all(&backup);
                Ok(())
            }
            Err(error) => {
                // Put the previous snapshot back before reporting the failure.
                let _ = std::fs::rename(&backup, output);
                Err(SnapshotError::Io {
                    path: output.to_path_buf(),
                    message: error.to_string(),
                })
            }
        }
    } else {
        std::fs::rename(staging, output).map_err(|error| SnapshotError::Io {
            path: output.to_path_buf(),
            message: error.to_string(),
        })
    }
}

/// Sibling directory a publish moves an existing output to before replacing it.
pub fn backup_dir(output: &Path) -> PathBuf {
    let mut name = output
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "snapshot".to_string());
    name.push_str(BACKUP_SUFFIX);
    output.with_file_name(name)
}

/// Read and structurally validate a manifest from a snapshot directory.
pub fn read_manifest(dir: &Path) -> Result<RepositoryManifest, SnapshotError> {
    let path = dir.join(MANIFEST_FILE);
    if !path.is_file() {
        return Err(SnapshotError::MissingManifest {
            dir: dir.to_path_buf(),
        });
    }
    let bytes = std::fs::read(&path).map_err(|error| SnapshotError::Io {
        path: path.clone(),
        message: error.to_string(),
    })?;
    let manifest: RepositoryManifest =
        serde_json::from_slice(&bytes).map_err(|error| SnapshotError::Json {
            path: path.clone(),
            message: error.to_string(),
        })?;
    validate_manifest(dir, &manifest)?;
    Ok(manifest)
}

/// Structural validation that does not require reading every artifact.
pub fn validate_manifest(dir: &Path, manifest: &RepositoryManifest) -> Result<(), SnapshotError> {
    if manifest.manifest_version != MANIFEST_VERSION {
        return Err(SnapshotError::UnsupportedManifestVersion {
            found: manifest.manifest_version,
            expected: MANIFEST_VERSION,
        });
    }
    if manifest.schema_version != manifest.config.schema_version {
        return Err(SnapshotError::InvalidManifest {
            reason: format!(
                "schema_version {} does not match config schema_version {}",
                manifest.schema_version, manifest.config.schema_version
            ),
        });
    }
    if !digest::is_digest(&manifest.analyzer_fingerprint) {
        return Err(SnapshotError::InvalidManifest {
            reason: format!(
                "analyzer_fingerprint `{}` is not a sha256 digest",
                manifest.analyzer_fingerprint
            ),
        });
    }
    if !digest::is_digest(&manifest.snapshot_digest) {
        return Err(SnapshotError::InvalidManifest {
            reason: format!(
                "snapshot_digest `{}` is not a sha256 digest",
                manifest.snapshot_digest
            ),
        });
    }
    let mut previous: Option<&str> = None;
    for file in &manifest.files {
        if let Some(previous) = previous {
            match previous.cmp(file.relative_path.as_str()) {
                std::cmp::Ordering::Less => {}
                std::cmp::Ordering::Equal => {
                    return Err(SnapshotError::DuplicatePath {
                        relative_path: file.relative_path.clone(),
                    })
                }
                std::cmp::Ordering::Greater => return Err(SnapshotError::UnsortedFiles),
            }
        }
        previous = Some(&file.relative_path);
        if file.relative_path.is_empty() {
            return Err(SnapshotError::InvalidManifest {
                reason: "a file record has an empty relative_path".to_string(),
            });
        }
        if !digest::is_digest(&file.content_digest) {
            return Err(SnapshotError::InvalidManifest {
                reason: format!(
                    "file `{}` has a malformed content_digest `{}`",
                    file.relative_path, file.content_digest
                ),
            });
        }
        if !digest::is_digest(&file.analysis_digest) {
            return Err(SnapshotError::InvalidManifest {
                reason: format!(
                    "file `{}` has a malformed analysis_digest `{}`",
                    file.relative_path, file.analysis_digest
                ),
            });
        }
        if file.object_key.is_empty() {
            return Err(SnapshotError::InvalidManifest {
                reason: format!("file `{}` has an empty object_key", file.relative_path),
            });
        }
    }
    let expected = manifest.compute_snapshot_digest();
    if expected != manifest.snapshot_digest {
        return Err(SnapshotError::InvalidManifest {
            reason: format!(
                "snapshot_digest {} does not match recomputed canonical digest {expected}",
                manifest.snapshot_digest
            ),
        });
    }
    let _ = dir;
    Ok(())
}

/// Read one file artifact from a snapshot directory.
pub fn read_file_analysis(dir: &Path, file: &IndexedFile) -> Result<FileAnalysis, SnapshotError> {
    let path = dir
        .join(FILES_DIR)
        .join(format!("{}.json", file.object_key));
    let bytes = std::fs::read(&path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            SnapshotError::MissingArtifact {
                object_key: file.object_key.clone(),
            }
        } else {
            SnapshotError::Io {
                path: path.clone(),
                message: error.to_string(),
            }
        }
    })?;
    let analysis: FileAnalysis =
        serde_json::from_slice(&bytes).map_err(|error| SnapshotError::Json {
            path: path.clone(),
            message: error.to_string(),
        })?;
    let found = digest::analysis_digest(&analysis);
    if found != file.analysis_digest {
        return Err(SnapshotError::ArtifactDigestMismatch {
            object_key: file.object_key.clone(),
            expected: file.analysis_digest.clone(),
            found,
        });
    }
    Ok(analysis)
}

/// Fully verify a snapshot: manifest structure, canonical ordering, snapshot
/// digest, and every referenced artifact's presence and digest.
///
/// A successful verification means the artifact is *internally consistent*. It
/// does **not** prove that the original source files still match it, because the
/// snapshot deliberately does not store the source.
pub fn verify(dir: &Path) -> Result<VerificationReport, SnapshotError> {
    let manifest = read_manifest(dir)?;
    let mut artifact_bytes = 0u64;
    for file in &manifest.files {
        let path = dir
            .join(FILES_DIR)
            .join(format!("{}.json", file.object_key));
        let metadata = std::fs::metadata(&path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                SnapshotError::MissingArtifact {
                    object_key: file.object_key.clone(),
                }
            } else {
                SnapshotError::Io {
                    path: path.clone(),
                    message: error.to_string(),
                }
            }
        })?;
        artifact_bytes += metadata.len();
        let analysis = read_file_analysis(dir, file)?;
        if analysis.file.relative_path != file.relative_path {
            return Err(SnapshotError::InvalidManifest {
                reason: format!(
                    "artifact `{}` holds path `{}` but the manifest says `{}`",
                    file.object_key, analysis.file.relative_path, file.relative_path
                ),
            });
        }
        if analysis.status.as_str() != file.analysis_status {
            return Err(SnapshotError::InvalidManifest {
                reason: format!(
                    "artifact `{}` status `{}` does not match manifest status `{}`",
                    file.object_key,
                    analysis.status.as_str(),
                    file.analysis_status
                ),
            });
        }
    }
    Ok(VerificationReport {
        manifest_version: manifest.manifest_version,
        snapshot_digest: manifest.snapshot_digest.clone(),
        files: manifest.files.len() as u64,
        checked_artifacts: manifest.files.len() as u64,
        artifact_bytes,
    })
}

/// Total on-disk size of a snapshot directory, in bytes.
pub fn artifact_size(dir: &Path) -> u64 {
    let mut total = 0u64;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            if metadata.is_dir() {
                stack.push(entry.path());
            } else {
                total += metadata.len();
            }
        }
    }
    total
}
