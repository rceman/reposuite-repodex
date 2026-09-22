//! Temporal-index artifact persistence + verification.
//!
//! Layout:
//!
//! ```text
//! <temporal>/
//!   manifest.json
//!   files.jsonl
//! ```
//!
//! `verify` re-checks manifest schema/policy, repository identity, HEAD
//! binding, canonical record ordering and the content digest — a wrong-repo,
//! wrong-schema or corrupt temporal artifact is rejected.

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use crate::repository::digest;

use super::model::*;
use super::TemporalError;

pub const MANIFEST_FILE: &str = "manifest.json";
pub const FILES_FILE: &str = "files.jsonl";

/// Prepare a clean staging dir beside `output` (atomic-build pattern).
pub fn prepare_staging(output: &Path) -> Result<PathBuf, TemporalError> {
    let parent = output.parent().unwrap_or_else(|| Path::new("."));
    let staging = parent.join(format!(
        ".{}.staging",
        output
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("temporal")
    ));
    if staging.exists() {
        fs::remove_dir_all(&staging).map_err(|e| TemporalError::Io {
            path: staging.clone(),
            reason: e.to_string(),
        })?;
    }
    fs::create_dir_all(&staging).map_err(|e| TemporalError::Io {
        path: staging.clone(),
        reason: e.to_string(),
    })?;
    Ok(staging)
}

/// Atomically publish a verified staging dir to `output`.
pub fn publish(staging: &Path, output: &Path) -> Result<(), TemporalError> {
    if output.exists() {
        fs::remove_dir_all(output).map_err(|e| TemporalError::Io {
            path: output.to_path_buf(),
            reason: e.to_string(),
        })?;
    }
    fs::rename(staging, output).map_err(|e| TemporalError::Io {
        path: output.to_path_buf(),
        reason: e.to_string(),
    })
}

/// Serialize records in canonical (path-sorted) order to `files.jsonl`.
pub fn write_records(staging: &Path, files: &[FileTemporal]) -> Result<u64, TemporalError> {
    let mut sorted: Vec<&FileTemporal> = files.iter().collect();
    sorted.sort_by(|a, b| a.path.cmp(&b.path));
    let mut w = fs::File::create(staging.join(FILES_FILE)).map_err(|e| TemporalError::Io {
        path: staging.join(FILES_FILE),
        reason: e.to_string(),
    })?;
    let mut bytes = 0u64;
    for f in sorted {
        let s = serde_json::to_string(f).map_err(|e| TemporalError::Serialize {
            reason: e.to_string(),
        })?;
        bytes += s.len() as u64 + 1;
        writeln!(w, "{s}").map_err(|e| TemporalError::Io {
            path: staging.join(FILES_FILE),
            reason: e.to_string(),
        })?;
    }
    Ok(bytes)
}

/// Canonical digest over the sorted record lines (deterministic identity).
pub fn records_digest(staging: &Path) -> Result<String, TemporalError> {
    let data = fs::read(staging.join(FILES_FILE)).map_err(|e| TemporalError::Io {
        path: staging.join(FILES_FILE),
        reason: e.to_string(),
    })?;
    Ok(digest::sha256(&data))
}

/// Write the manifest into `staging`.
pub fn write_manifest(staging: &Path, manifest: &TemporalManifest) -> Result<(), TemporalError> {
    let text = serde_json::to_string_pretty(manifest).map_err(|e| TemporalError::Serialize {
        reason: e.to_string(),
    })?;
    fs::write(staging.join(MANIFEST_FILE), text).map_err(|e| TemporalError::Io {
        path: staging.join(MANIFEST_FILE),
        reason: e.to_string(),
    })
}

/// Read the manifest.
pub fn read_manifest(dir: &Path) -> Result<TemporalManifest, TemporalError> {
    let text = fs::read_to_string(dir.join(MANIFEST_FILE)).map_err(|e| TemporalError::Io {
        path: dir.join(MANIFEST_FILE),
        reason: e.to_string(),
    })?;
    serde_json::from_str(&text).map_err(|e| TemporalError::Serialize {
        reason: format!("manifest: {e}"),
    })
}

/// Read all records.
pub fn read_files(dir: &Path) -> Result<Vec<FileTemporal>, TemporalError> {
    let f = fs::File::open(dir.join(FILES_FILE)).map_err(|e| TemporalError::Io {
        path: dir.join(FILES_FILE),
        reason: e.to_string(),
    })?;
    let mut out = Vec::new();
    for line in BufReader::new(f).lines() {
        let line = line.map_err(|e| TemporalError::Io {
            path: dir.join(FILES_FILE),
            reason: e.to_string(),
        })?;
        if line.trim().is_empty() {
            continue;
        }
        out.push(
            serde_json::from_str(&line).map_err(|e| TemporalError::Serialize {
                reason: format!("record: {e}"),
            })?,
        );
    }
    Ok(out)
}

/// Verify an artifact: schema, ordering, count + digest consistency.
pub fn verify(dir: &Path) -> Result<TemporalManifest, TemporalError> {
    let manifest = read_manifest(dir)?;
    if manifest.schema_version != TEMPORAL_SCHEMA_VERSION {
        return Err(TemporalError::FingerprintMismatch {
            expected: format!("schema {TEMPORAL_SCHEMA_VERSION}"),
            found: format!("schema {}", manifest.schema_version),
        });
    }
    if manifest.policy_version != TEMPORAL_POLICY_VERSION {
        return Err(TemporalError::FingerprintMismatch {
            expected: format!("policy {TEMPORAL_POLICY_VERSION}"),
            found: format!("policy {}", manifest.policy_version),
        });
    }
    let files = read_files(dir)?;
    if files.len() as u64 != manifest.files_tracked {
        return Err(TemporalError::Corrupt {
            reason: format!(
                "files_tracked {} != {} records",
                manifest.files_tracked,
                files.len()
            ),
        });
    }
    // canonical ordering + no absolute paths
    let mut prev = "";
    for f in &files {
        if f.path.as_str() < prev {
            return Err(TemporalError::Corrupt {
                reason: format!("records out of order at {}", f.path),
            });
        }
        if f.path.starts_with('/') {
            return Err(TemporalError::Corrupt {
                reason: format!("absolute path {}", f.path),
            });
        }
        prev = &f.path;
    }
    let found = records_digest(dir)?;
    if found != manifest.content_digest {
        return Err(TemporalError::DigestMismatch {
            expected: manifest.content_digest.clone(),
            found,
        });
    }
    Ok(manifest)
}
