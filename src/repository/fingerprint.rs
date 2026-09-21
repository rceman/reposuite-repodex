//! Analyzer and analysis-configuration identity.
//!
//! Reusing a previously persisted normalized analysis is only safe when the
//! bytes are identical **and** the code that produced the facts is identical.
//! Source equality alone is not enough: after a grammar bump or an extraction
//! policy change, the same bytes must produce new facts.
//!
//! Two identities are recorded:
//!
//! * [`AnalyzerFingerprint`] — what the extraction pipeline is. It must change
//!   whenever normalized facts for unchanged bytes could change.
//! * [`SnapshotConfig`] — the analysis configuration that decides *which* files
//!   are analyzed and *how* (size limit, ignore policy, language set).

use serde::{Deserialize, Serialize};

use crate::model::{LanguageId, SCHEMA_VERSION};
use crate::parser::ParserRegistry;
use crate::repository::digest;

/// Version of the analysis/extraction pipeline.
///
/// **This constant must be incremented whenever any of the following changes in
/// a way that could alter the normalized facts produced from unchanged source
/// bytes:**
///
/// * the shape or meaning of any normalized fact (see also [`SCHEMA_VERSION`]);
/// * an adapter's extraction logic, including which occurrences are recognized
///   and how their ranges are computed;
/// * the recovery scan, the match-limit policy or the diagnostic set;
/// * the input validation rules that decide a file's analysis status.
///
/// It does **not** need to change for documentation, tests, CLI formatting or
/// anything else that cannot affect facts.
///
/// * `1` — the original fact set.
/// * `2` — the Rust adapter now emits `LocalBindingOccurrence` facts for local
///   name bindings (`let`, parameters, `for`/`match`/`if let`/`while let`
///   patterns) with bounded visibility ranges.
/// * `3` — the Go adapter now emits `LocalBindingOccurrence` facts for
///   function-local bindings (parameters, named results, method receivers,
///   function-literal parameters/results, `:=` short variables, `var`/`const`/
///   `type` declarations, `for`/`range`/`switch`/`select`/`if`/`type-switch`
///   bindings) with bounded visibility ranges, and now descends into
///   `var`/`const`/`type` declaration values so nested `func` literals (and the
///   calls inside them) are reachable everywhere.
pub const ANALYSIS_ABI_VERSION: u32 = 3;

/// Version of the deterministic discovery/ignore policy.
///
/// Increment when the set of files a scan considers changes: the pruned
/// directory list, the `.gitignore` handling, or the extension-to-language
/// mapping.
pub const IGNORE_POLICY_VERSION: u32 = 1;

/// Version of the on-disk snapshot manifest format.
///
/// Increment when the manifest's field set or meaning changes. This is the
/// artifact format version, not the fact schema version.
pub const MANIFEST_VERSION: u32 = 1;

/// Identity of the extraction pipeline.
///
/// Two snapshots built by different analyzers must never share a reused
/// analysis. The fingerprint covers the fact schema, the analysis ABI version,
/// the Tree-sitter runtime ABI and every grammar version, so a grammar bump or
/// an extraction change invalidates reuse without anyone remembering to bump a
/// single number by hand for the grammar case.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnalyzerFingerprint {
    /// Canonical, human-readable fingerprint text.
    pub text: String,
    /// SHA-256 of [`AnalyzerFingerprint::text`].
    pub digest: String,
}

impl AnalyzerFingerprint {
    /// Compute the fingerprint of the analyzer built from `registry`.
    pub fn current(registry: &ParserRegistry) -> Self {
        let mut text = format!(
            "schema={SCHEMA_VERSION} abi={ANALYSIS_ABI_VERSION} \
             ts_language_version={} ts_min_compatible={}",
            tree_sitter::LANGUAGE_VERSION,
            tree_sitter::MIN_COMPATIBLE_LANGUAGE_VERSION,
        );
        // Grammars in the fixed language order, so the text is deterministic.
        for language in LanguageId::ALL {
            let grammar = registry.adapter(language).grammar();
            text.push_str(&format!(
                " {}={}:{}",
                language.as_str(),
                grammar.crate_name,
                grammar.crate_version
            ));
        }
        let digest = digest::sha256_text(&text);
        Self { text, digest }
    }

    /// The value stored in the manifest: the digest, not the free text.
    pub fn recorded(&self) -> &str {
        &self.digest
    }
}

/// The analysis configuration that decides which files are analyzed and how.
///
/// This is part of the reuse contract: a previous snapshot built under a
/// different configuration cannot be trusted to contain the right file set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotConfig {
    /// Supported language identifiers, in the fixed language order.
    pub languages: Vec<String>,
    /// Maximum source file size in bytes.
    pub max_file_size: u64,
    /// Whether `.gitignore` files inside the root are honoured.
    pub respect_gitignore: bool,
    /// Discovery/ignore policy version.
    pub ignore_policy_version: u32,
    /// Normalized fact schema version.
    pub schema_version: u32,
}

impl SnapshotConfig {
    /// Build the configuration identity for the given scan settings.
    pub fn new(max_file_size: u64, respect_gitignore: bool) -> Self {
        Self {
            languages: LanguageId::ALL
                .iter()
                .map(|language| language.as_str().to_string())
                .collect(),
            max_file_size,
            respect_gitignore,
            ignore_policy_version: IGNORE_POLICY_VERSION,
            schema_version: SCHEMA_VERSION,
        }
    }

    /// Canonical text used by the snapshot digest and by equality checks.
    pub fn canonical_text(&self) -> String {
        format!(
            "languages={} max_file_size={} respect_gitignore={} ignore_policy={} schema={}",
            self.languages.join(","),
            self.max_file_size,
            self.respect_gitignore,
            self.ignore_policy_version,
            self.schema_version,
        )
    }
}
