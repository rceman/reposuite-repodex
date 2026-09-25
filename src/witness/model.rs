//! CurrentSourceWitness — the exact current source bytes for a validated
//! evidence range (§5-§13). A locator/range is navigation evidence; this is
//! the actual content the locator points to. Never synthesized from history.

use serde::Serialize;

/// Witness schema identity.
pub const WITNESS_SCHEMA: &str = "reposuite.repodex.source-witness.v1";

/// Objective witness role (§6) — only roles RepoDex can already identify.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WitnessRole {
    Declaration,
    DeclarationHeader,
    Body,
    RelationOccurrence,
    TestDeclaration,
    ConfigOccurrence,
}

/// The exact current source bytes for one bounded range in the validated view.
#[derive(Debug, Clone, Serialize)]
pub struct CurrentSourceWitness {
    pub schema: String,
    /// Canonical project scope + validated view identity.
    pub scope: String,
    pub repository_view: String,
    pub repo_relative_path: String,
    pub role: WitnessRole,
    /// `fact` | `candidate` — preserved, never upgraded by exact bytes (§12).
    pub evidence_class: String,
    /// Entity/relation identity where available.
    pub entity: String,
    /// Digest of the exact file content the bytes were sliced from (§7).
    pub current_file_digest: String,
    pub byte_start: u32,
    pub byte_end: u32,
    pub line_start: u32,
    pub line_end: u32,
    /// The exact current source bytes (UTF-8 lossy-safe).
    pub source: String,
    /// True when the delivered range covers the full obligation range; false
    /// when truncated (§9).
    pub complete_for_range: bool,
    pub truncated: bool,
    /// Provenance: index snapshot object + rule that produced the range.
    pub provenance: String,
}
