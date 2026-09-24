//! SymbolExposure — derived, rebuildable intelligence (§1-§2, §27).
//!
//! Proves that source bytes/ranges associated with a *symbol* were contained in
//! source content exposed to an Agent. It does NOT claim the model attended to,
//! reasoned about, or understood the symbol — those stay separate
//! interpretations (§1). Built from `SourceObserved` + RepoDex parsed symbol
//! intelligence; harness-neutral.

use serde::{Deserialize, Serialize};

use crate::agent_event::model::ObservationKind;

/// SymbolExposure resolver/schema version — bump on semantic change (§28).
pub const SYMBOL_EXPOSURE_SCHEMA_VERSION: u32 = 1;

/// How the observed range relates to a symbol's source range (§24).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverlapClass {
    /// The whole symbol declaration is inside the observed range.
    SymbolInsideObserved,
    /// The observed range is fully inside the symbol's range.
    ObservedInsideSymbol,
    /// The two ranges share bytes but neither contains the other.
    Partial,
    /// A discrete reference/call occurrence range sits inside the observed
    /// range (used only for `reference_occurrence`).
    OccurrenceInsideObserved,
}

impl OverlapClass {
    pub fn as_str(self) -> &'static str {
        match self {
            OverlapClass::SymbolInsideObserved => "symbol_inside_observed",
            OverlapClass::ObservedInsideSymbol => "observed_inside_symbol",
            OverlapClass::Partial => "partial",
            OverlapClass::OccurrenceInsideObserved => "occurrence_inside_observed",
        }
    }
}

/// Required exposure kinds (§18-§21). Semantics are explicit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExposureKind {
    /// The observed source lies inside the symbol's declaration range (the
    /// body/surrounding construct was shown) (§19).
    EnclosingDeclaration,
    /// The declaration's own header/name range overlaps the observed source —
    /// the declaration source itself was actually present (§20). Stronger.
    DeclarationOccurrence,
    /// The observed source contains a reference/call occurrence to a symbol
    /// (§21). Target certainty is separate (`Certainty`).
    ReferenceOccurrence,
}

impl ExposureKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ExposureKind::EnclosingDeclaration => "enclosing_declaration",
            ExposureKind::DeclarationOccurrence => "declaration_occurrence",
            ExposureKind::ReferenceOccurrence => "reference_occurrence",
        }
    }
}

/// FACT / CANDIDATE certainty for a referenced/exposed symbol (§22).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Certainty {
    /// Resolved to a concrete RepoDex declaration/entity identity.
    Fact,
    /// Syntactic/bounded reference — the target is a written name or a
    /// candidate set, not proven symbol truth.
    Candidate,
}

/// Source-precision actually used to bind this exposure (§8). Never upgraded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RangePrecision {
    Byte,
    LineColumn,
    Line,
    WholeFile,
}

impl RangePrecision {
    pub fn as_str(self) -> &'static str {
        match self {
            RangePrecision::Byte => "byte",
            RangePrecision::LineColumn => "line_column",
            RangePrecision::Line => "line",
            RangePrecision::WholeFile => "whole_file",
        }
    }
}

/// Resolution outcome for one SourceObserved -> SymbolExposure attempt (§31).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionState {
    /// Bound to the exact content version; symbols resolved.
    Resolved,
    /// Some symbol facts produced but coverage was incomplete (e.g. weak
    /// language support, recovery regions).
    Partial,
    /// Observed digest differs from index and no source bytes/analysis exist
    /// for that version — never resolved against the wrong file (§30).
    UnresolvedMissingSourceVersion,
    /// The file's language has no symbol facts to expose.
    UnresolvedUnsupportedLanguage,
    /// Source version bound but no symbol range overlapped the observation.
    UnresolvedNoSymbolOverlap,
    /// No file-content version identity was available; resolved against the
    /// indexed/current version only (read-only corpus assumption — §45/§46).
    AssumedIndexedVersion,
}

/// One symbol-level exposure derived from one `SourceObserved` event (§17).
///
/// Per-investigation constants (`investigation_id`, `repository_id`,
/// `repo_head`) are carried on the parent `SymbolExposureDoc` rather than
/// repeated on every record — normalized provenance (§75).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SymbolExposure {
    /// Deterministic exposure id: `sexp-{session_id}-{seq}-{symbol_id}`.
    pub exposure_id: String,
    /// Session this exposure belongs to (an investigation may span sessions).
    pub session_id: String,
    /// Sequence of the `source_observed` event this exposure came from.
    pub source_event_sequence: u64,
    pub path: String,
    /// Authoritative file-content version (`sha256:` content digest == the
    /// `IndexedFile.content_digest` of the version whose symbol map was used).
    pub file_content_digest: Option<String>,
    /// Stable RepoDex symbol/entity identity (`decl:{path}#{declaration_id}`).
    /// NOTE: file-local ordinal — position-unstable across edits (§9). Use
    /// `symbol_locator` for cross-version identity/rebinding.
    pub symbol_id: String,
    /// Stable qualified declaration locator `{lang}:{path}:{kind}:{qname}`
    /// (name+scope-based, survives insertions). Set for declaration/enclosing
    /// exposures; None for bare references.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol_locator: Option<String>,
    /// Intrinsic decl facets captured at exposure (§11); `None` when the source
    /// bytes for that version were unavailable (§13).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub facets: Option<crate::memory::rebind::DeclFacets>,
    pub symbol_name: String,
    pub symbol_kind: String,
    /// Observed source range (line precision unless bytes known).
    pub observed_line_start: Option<u64>,
    pub observed_line_end: Option<u64>,
    /// The symbol's own source range (row/col/byte from the analysis).
    pub symbol_line_start: u32,
    pub symbol_line_end: u32,
    pub overlap: OverlapClass,
    pub exposure_kind: ExposureKind,
    /// Originating observation provenance (explicit_read/search_snippet/...).
    pub observation_provenance: ObservationKind,
    /// FACT/CANDIDATE for the exposed symbol (references keep uncertainty).
    pub certainty: Certainty,
    /// For `reference_occurrence`: the referenced symbol identity when resolved
    /// (`decl:` key) or the written candidate name.
    pub reference_target: Option<String>,
    /// Nesting depth for `enclosing_declaration` (0 = innermost) (§23).
    pub depth: u32,
    /// True for the innermost enclosing declaration.
    pub innermost: bool,
    /// Resolution diagnostics for this exposure.
    pub resolution: ResolutionState,
    /// Source precision actually used.
    pub precision: RangePrecision,
    pub schema_version: u32,
}

/// A per-observation resolution outcome record (diagnostics, §80).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolutionRecord {
    pub source_event_sequence: u64,
    pub session_id: String,
    pub investigation_id: String,
    pub path: String,
    pub file_content_digest: Option<String>,
    /// How the symbol map was obtained.
    pub source: ResolutionSource,
    pub resolution: ResolutionState,
    pub exposures: usize,
}

/// How the symbol map for a (path, content digest) was obtained (§57, §80).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionSource {
    /// Observed digest matched the indexed version — reused, no parse (§9).
    IndexedHit,
    /// Digest differed but the version's symbol map was already cached.
    CachedVersion,
    /// Digest differed; source bytes were available and parsed once.
    NewParse,
    /// No version identity supplied — bound to the indexed/current file.
    AssumedIndexed,
    /// Could not be resolved to any usable symbol map.
    Unresolved,
}

impl ResolutionSource {
    pub fn as_str(self) -> &'static str {
        match self {
            ResolutionSource::IndexedHit => "indexed_hit",
            ResolutionSource::CachedVersion => "cached_version",
            ResolutionSource::NewParse => "new_parse",
            ResolutionSource::AssumedIndexed => "assumed_indexed",
            ResolutionSource::Unresolved => "unresolved",
        }
    }
}
