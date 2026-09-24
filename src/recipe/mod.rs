//! Guarded Evidence Recipes V1: bounded, version-safe evidence routes mined
//! from repeated investigation motifs and re-executed against the current
//! validated view. Read-only, deterministic, no model.

pub mod exec;
pub mod mine;
pub mod model;
pub mod store;

pub use exec::{execute, match_recipe, RecipeOutcome, StepTrace};
pub use mine::{mine, MIN_SUPPORT};
pub use model::{
    RecipeDefinition, RecipeFamily, RecipeMatch, Selector, SelectorStep, RECIPE_SCHEMA,
    RECIPE_VERSION,
};
pub use store::{RecipeError, RecipeStore};

/// Recipe query policy (§41): `off` | `auto` | `force` (debug).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecipePolicy {
    Off,
    Auto,
    Force,
}
impl RecipePolicy {
    pub fn parse(s: &str) -> Self {
        match s {
            "auto" => Self::Auto,
            "force" => Self::Force,
            _ => Self::Off,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Auto => "auto",
            Self::Force => "force",
        }
    }
}
