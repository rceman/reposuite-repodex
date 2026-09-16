//! Reproducible digests over complete canonical facts.
//!
//! TASK 2 needs to compare complete canonical facts, or a digest derived from
//! them, instead of aggregate counters. Everything here is computed from
//! [`crate::model::FileAnalysis::canonical_lines`], which contains no timing
//! metadata, no absolute paths and no random identifiers.

use crate::model::FileAnalysis;
use crate::scanner::ScanReport;

/// FNV-1a 64-bit digest rendered as `fnv1a64:<hex>`.
///
/// This is a change detector for canonical fact text, not a cryptographic hash.
pub fn digest(text: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("fnv1a64:{hash:016x}")
}

/// Digest of one file's complete canonical facts.
pub fn analysis_digest(analysis: &FileAnalysis) -> String {
    digest(&analysis.canonical_text())
}

/// Digest of every analyzed file's canonical facts, ordered by relative path.
///
/// Files that were never analyzed (unsupported extension, read failure) carry
/// no facts and therefore contribute nothing beyond their path and status.
pub fn scan_digest(report: &ScanReport) -> String {
    let mut text = String::new();
    for file in &report.files {
        text.push_str(&file.relative_path);
        text.push(' ');
        text.push_str(file.outcome.as_str());
        text.push('\n');
        if let Some(analysis) = &file.analysis {
            text.push_str(&analysis.canonical_text());
        }
    }
    digest(&text)
}

/// Full canonical fact text of every analyzed file, ordered by relative path.
pub fn scan_canonical_text(report: &ScanReport) -> String {
    let mut text = String::new();
    for file in &report.files {
        if let Some(analysis) = &file.analysis {
            text.push_str(&analysis.canonical_text());
        }
    }
    text
}
