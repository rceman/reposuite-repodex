//! Guarded Evidence Recipe model (Part B1-B2, §12-§15, §19-§25).
//!
//! A recipe is NOT a cached answer — it is a bounded, version-safe *plan* for
//! reconstructing current evidence for a recurring investigation shape. It is
//! read-only, typed, deterministic, and contains no executable code (§11).

use serde::{Deserialize, Serialize};

/// Recipe schema identity — bump on incompatible change.
pub const RECIPE_SCHEMA: &str = "reposuite.repodex.recipe.v1";
/// Recipe version (selector-policy version).
pub const RECIPE_VERSION: u32 = 1;

/// A typed, bounded selector operation (§13-§14). These map to operations the
/// query engine already supports — no arbitrary code/shell/SQL (§11).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Selector {
    /// Resolve the anchor declaration from the query (parameterized).
    ResolveAnchor,
    /// Rebind a remembered symbol anchor via the current view.
    ResolveMemoryAnchor { locator: String },
    /// Callers of the anchor.
    CallersOf,
    /// Callees of the anchor.
    CalleesOf,
    /// One-hop related entities of the anchor.
    RelatedOf,
    /// Bounded two-endpoint path from anchor to `to`.
    PathTo { to: String },
    /// Deterministic test candidates for the anchor.
    TestCandidatesOf,
    /// Owning package/module of the anchor.
    OwningModuleOf,
}

/// Recipe family (§44) — small objective set only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecipeFamily {
    DefinitionToCallers,
    DefinitionToCallees,
    DefinitionToTestCandidates,
    TwoAnchorConnector,
    DefinitionToPackageModule,
}

/// Recipe applicability result (§17).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecipeMatch {
    Match,
    Ambiguous,
    NoMatch,
}

/// A bounded, version-safe evidence recipe.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecipeDefinition {
    pub schema: String,
    pub recipe_id: String,
    /// Canonical project scope (shared across worktrees/branches; §5).
    pub project_scope: String,
    pub version: u32,
    pub family: RecipeFamily,
    /// Query-family matcher: normalized terms/identifiers the recipe applies to.
    /// Conservative — similarity is a hint, never proof (§16-§17).
    pub query_family_terms: Vec<String>,
    /// Ordered selector steps (a small list; partial order via `after`).
    pub steps: Vec<SelectorStep>,
    /// Minimum support the recipe was mined with (provenance, §31).
    pub support: u32,
    /// Producer/provenance (episode ids that contributed).
    pub provenance: Vec<String>,
}

/// One selector step with an optional dependency (`after` = step index it
/// depends on; steps with the same `after` may run in any order, §15).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectorStep {
    pub id: u32,
    pub selector: Selector,
    /// Step id this step depends on (None = independent/root).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<u32>,
}
