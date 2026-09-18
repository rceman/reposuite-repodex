//! The repository snapshot manifest.
//!
//! The manifest is the canonical, human-inspectable description of one
//! repository snapshot: what the analyzer was, what configuration produced it,
//! every indexed file with its content digest, analysis status and analysis
//! digest, and the repository-level totals. It contains no absolute paths, no
//! timestamps and no random identifiers.

use serde::{Deserialize, Serialize};

use crate::repository::digest;
use crate::repository::fingerprint::{AnalyzerFingerprint, SnapshotConfig, MANIFEST_VERSION};

/// One indexed supported file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexedFile {
    /// `/`-separated path relative to the repository root.
    pub relative_path: String,
    /// Stable language identifier.
    pub language: String,
    /// Number of source bytes read for this file. For an over-limit file this
    /// is the bounded prefix that was actually read, not the true file size.
    pub source_bytes: u64,
    /// True when the read stopped at the size limit, so `source_bytes` is a
    /// prefix rather than the whole file.
    pub source_truncated: bool,
    /// SHA-256 of the bytes read. Authoritative content identity.
    pub content_digest: String,
    /// Normalized analysis status (`clean`, `recovered`, `incomplete`,
    /// `unsupported`, `failed`).
    pub analysis_status: String,
    /// SHA-256 of the file's canonical normalized facts.
    pub analysis_digest: String,
    /// Stable key of this file's artifact under `files/`.
    pub object_key: String,
    /// Rendered diagnostics, one per entry.
    pub diagnostics: Vec<String>,
    pub declarations: u64,
    pub imports: u64,
    pub references: u64,
    pub call_like: u64,
    pub test_candidates: u64,
}

impl IndexedFile {
    /// Canonical one-line rendering used by the snapshot digest.
    fn canonical_line(&self) -> String {
        format!(
            "file path={} lang={} bytes={} truncated={} content={} status={} analysis={} \
             counts={},{},{},{},{}",
            self.relative_path,
            self.language,
            self.source_bytes,
            self.source_truncated,
            self.content_digest,
            self.analysis_status,
            self.analysis_digest,
            self.declarations,
            self.imports,
            self.references,
            self.call_like,
            self.test_candidates,
        )
    }
}

/// Repository-level coverage summary.
///
/// This is deliberately not a single "complete" flag. A snapshot can cover the
/// whole subtree and still contain files whose analysis is recovered, partial or
/// failed, and callers must be able to tell those apart.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoverageSummary {
    /// Files visited by the walk (supported or not).
    pub visited_files: u64,
    /// Visited files whose extension is a supported language.
    pub supported_files: u64,
    /// Visited files whose extension is not supported.
    pub unsupported_files: u64,
    /// Indexed files whose analysis is clean.
    pub clean: u64,
    /// Indexed files whose analysis recovered from syntax damage.
    pub recovered: u64,
    /// Indexed files whose analysis is known to be partial.
    pub incomplete: u64,
    /// Indexed files whose analysis failed (read/grammar/query failure).
    pub failed: u64,
    /// Indexed files that were not analyzed (size or encoding).
    pub skipped: u64,
    /// Paths the walk could not enter or stat.
    pub traversal_failures: u64,
    /// True when the walk covered the whole subtree under the root.
    pub scan_complete: bool,
}

impl CoverageSummary {
    /// True when the walk covered the whole subtree and every indexed file
    /// produced a complete analysis.
    ///
    /// "Complete" here means clean **or** recovered: a recovered analysis ran
    /// over the whole file and produced its full fact set, it just also recorded
    /// that Tree-sitter had to repair damaged syntax. A partial (`incomplete`)
    /// or failed analysis, or a file that was skipped entirely, makes this
    /// false. Recovered is still not clean — see
    /// [`CoverageSummary::is_fully_clean`].
    pub fn is_fully_analyzed(&self) -> bool {
        self.scan_complete && self.incomplete == 0 && self.failed == 0 && self.skipped == 0
    }

    /// True when every indexed file is additionally free of recovery artifacts.
    pub fn is_fully_clean(&self) -> bool {
        self.is_fully_analyzed() && self.recovered == 0
    }
}

/// Repository-level fact totals.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FactTotals {
    pub declarations: u64,
    pub imports: u64,
    pub references: u64,
    pub call_like: u64,
    pub test_candidates: u64,
    /// Sum of `IndexedFile::source_bytes` over indexed files.
    pub source_bytes: u64,
}

/// The canonical repository snapshot manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepositoryManifest {
    /// Artifact format version.
    pub manifest_version: u32,
    /// Normalized fact schema version.
    pub schema_version: u32,
    /// Digest of the analyzer that produced this snapshot.
    pub analyzer_fingerprint: String,
    /// Human-readable analyzer fingerprint text (informational; not digested).
    #[serde(default)]
    pub analyzer_fingerprint_text: String,
    /// Analysis configuration identity.
    pub config: SnapshotConfig,
    /// Deterministic digest over the canonical snapshot content.
    pub snapshot_digest: String,
    pub coverage: CoverageSummary,
    pub totals: FactTotals,
    /// Indexed files, sorted by `relative_path`.
    pub files: Vec<IndexedFile>,
}

impl RepositoryManifest {
    /// Canonical text over everything the snapshot digest covers.
    ///
    /// Excludes the fingerprint free text and the snapshot digest itself, so the
    /// digest is a pure function of the analyzer identity, the configuration and
    /// the indexed file records.
    pub fn canonical_text(&self) -> String {
        let mut text = String::new();
        text.push_str(&format!(
            "snapshot manifest_version={} schema={} fingerprint={} config={}\n",
            self.manifest_version,
            self.schema_version,
            self.analyzer_fingerprint,
            self.config.canonical_text(),
        ));
        for file in &self.files {
            text.push_str(&file.canonical_line());
            text.push('\n');
        }
        text
    }

    /// Recompute the snapshot digest from canonical content.
    pub fn compute_snapshot_digest(&self) -> String {
        digest::sha256_text(&self.canonical_text())
    }

    /// Recompute the snapshot digest and store it.
    pub fn refresh_snapshot_digest(&mut self) {
        self.snapshot_digest = self.compute_snapshot_digest();
    }

    /// Look up one indexed file by relative path.
    pub fn file(&self, relative_path: &str) -> Option<&IndexedFile> {
        self.files
            .binary_search_by(|file| file.relative_path.as_str().cmp(relative_path))
            .ok()
            .map(|index| &self.files[index])
    }

    /// True when the recorded analyzer fingerprint matches `fingerprint`.
    pub fn fingerprint_matches(&self, fingerprint: &AnalyzerFingerprint) -> bool {
        self.analyzer_fingerprint == fingerprint.digest
    }

    /// True when the recorded configuration matches `config`.
    pub fn config_matches(&self, config: &SnapshotConfig) -> bool {
        self.config == *config
    }

    /// Build a manifest skeleton for a new snapshot.
    pub fn new(fingerprint: &AnalyzerFingerprint, config: SnapshotConfig) -> Self {
        Self {
            manifest_version: MANIFEST_VERSION,
            schema_version: config.schema_version,
            analyzer_fingerprint: fingerprint.digest.clone(),
            analyzer_fingerprint_text: fingerprint.text.clone(),
            config,
            snapshot_digest: String::new(),
            coverage: CoverageSummary::default(),
            totals: FactTotals::default(),
            files: Vec::new(),
        }
    }
}
