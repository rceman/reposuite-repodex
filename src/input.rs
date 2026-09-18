//! Bounded file input.
//!
//! TASK 1 analyzes UTF-8 source. Input handling is bounded by an explicit
//! maximum size that is enforced while reading, not only from file metadata.

use std::fs::File;
use std::io::{ErrorKind, Read};
use std::path::Path;

use crate::model::{AnalysisStatus, Diagnostic, DiagnosticKind};

/// Default maximum source file size: 8 MiB.
pub const DEFAULT_MAX_FILE_SIZE: u64 = 8 * 1024 * 1024;

/// Hard ceiling on the configurable maximum size.
///
/// Byte offsets in the normalized model are `u32`, so a snapshot can never be
/// larger than 4 GiB.
pub const MAX_SUPPORTED_FILE_SIZE: u64 = u32::MAX as u64;

/// A successfully read source buffer.
#[derive(Debug)]
pub struct SourceInput {
    pub bytes: Vec<u8>,
}

/// Why a file could not be turned into analyzable source.
#[derive(Debug)]
pub enum InputError {
    /// The file was not there (or vanished between discovery and reading).
    NotFound,
    /// The file could not be read for another reason.
    Io(String),
    /// The file exceeds the configured maximum size.
    ///
    /// `observed` is the number of bytes read before the limit was detected, so
    /// it is a lower bound on the real file size rather than the file size: the
    /// read stops as soon as the limit is exceeded.
    TooLarge { limit: u64, observed: u64 },
    /// The bytes are not valid UTF-8.
    NotUtf8 { valid_up_to: usize },
}

impl InputError {
    pub fn status(&self) -> AnalysisStatus {
        match self {
            InputError::NotFound | InputError::Io(_) => AnalysisStatus::Failed,
            InputError::TooLarge { .. } | InputError::NotUtf8 { .. } => AnalysisStatus::Unsupported,
        }
    }

    pub fn diagnostic(&self) -> Diagnostic {
        match self {
            InputError::NotFound => Diagnostic::error(
                DiagnosticKind::DisappearedDuringScan,
                "file disappeared before it could be read",
            ),
            InputError::Io(message) => {
                Diagnostic::error(DiagnosticKind::IoError, format!("read failed: {message}"))
            }
            InputError::TooLarge { limit, observed } => Diagnostic::error(
                DiagnosticKind::FileTooLarge,
                format!(
                    "file is larger than the configured maximum of {limit} bytes \
                     (the read was truncated after {observed} bytes)"
                ),
            ),
            InputError::NotUtf8 { valid_up_to } => Diagnostic::error(
                DiagnosticKind::UnsupportedEncoding,
                format!("input is not valid UTF-8 (first invalid byte at offset {valid_up_to})"),
            ),
        }
    }
}

/// Read at most `max_bytes` from `path`.
///
/// The read is bounded by `max_bytes + 1` bytes so that oversized files are
/// detected even when metadata is missing, stale or lying.
pub fn read_source(path: &Path, max_bytes: u64) -> Result<SourceInput, InputError> {
    let limit = max_bytes.min(MAX_SUPPORTED_FILE_SIZE);
    let file = File::open(path).map_err(|error| match error.kind() {
        ErrorKind::NotFound => InputError::NotFound,
        _ => InputError::Io(error.to_string()),
    })?;
    let mut reader = file.take(limit + 1);
    let mut bytes = Vec::new();
    reader
        .read_to_end(&mut bytes)
        .map_err(|error| InputError::Io(error.to_string()))?;
    if bytes.len() as u64 > limit {
        return Err(InputError::TooLarge {
            limit,
            observed: bytes.len() as u64,
        });
    }
    match std::str::from_utf8(&bytes) {
        Ok(_) => Ok(SourceInput { bytes }),
        Err(error) => Err(InputError::NotUtf8 {
            valid_up_to: error.valid_up_to(),
        }),
    }
}

/// A bounded read that keeps the bytes it managed to read.
///
/// [`read_source`] discards the buffer on failure, which is right for analysis
/// but wrong for repository indexing: the snapshot must record a content digest
/// of exactly the bytes that were read, including for files that were skipped
/// because they are over the size limit or not valid UTF-8.
#[derive(Debug)]
pub struct BoundedRead {
    /// The bytes that were read (empty when the file could not be opened).
    pub bytes: Vec<u8>,
    /// True when the read stopped at the size limit, so `bytes` is a prefix.
    pub truncated: bool,
    /// The validation failure, if any.
    pub error: Option<InputError>,
}

/// Read at most `max_bytes + 1` bytes and report the outcome without discarding
/// the buffer.
pub fn read_bounded(path: &Path, max_bytes: u64) -> BoundedRead {
    let limit = max_bytes.min(MAX_SUPPORTED_FILE_SIZE);
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) => {
            let error = match error.kind() {
                ErrorKind::NotFound => InputError::NotFound,
                _ => InputError::Io(error.to_string()),
            };
            return BoundedRead {
                bytes: Vec::new(),
                truncated: false,
                error: Some(error),
            };
        }
    };
    let mut reader = file.take(limit + 1);
    let mut bytes = Vec::new();
    if let Err(error) = reader.read_to_end(&mut bytes) {
        return BoundedRead {
            bytes,
            truncated: false,
            error: Some(InputError::Io(error.to_string())),
        };
    }
    if bytes.len() as u64 > limit {
        let observed = bytes.len() as u64;
        return BoundedRead {
            bytes,
            truncated: true,
            error: Some(InputError::TooLarge { limit, observed }),
        };
    }
    let error = std::str::from_utf8(&bytes)
        .err()
        .map(|error| InputError::NotUtf8 {
            valid_up_to: error.valid_up_to(),
        });
    BoundedRead {
        bytes,
        truncated: false,
        error,
    }
}

/// Validate a byte buffer the same way [`read_source`] does.
pub fn validate_bytes(bytes: Vec<u8>, max_bytes: u64) -> Result<SourceInput, InputError> {
    let limit = max_bytes.min(MAX_SUPPORTED_FILE_SIZE);
    if bytes.len() as u64 > limit {
        return Err(InputError::TooLarge {
            limit,
            observed: bytes.len() as u64,
        });
    }
    match std::str::from_utf8(&bytes) {
        Ok(_) => Ok(SourceInput { bytes }),
        Err(error) => Err(InputError::NotUtf8 {
            valid_up_to: error.valid_up_to(),
        }),
    }
}
