//! Deterministic recipe mining (Part B5-B6, §29-§36).
//!
//! Recipes are mined from repeated investigation MOTIFS — episodes that share
//! query-family identifier terms AND the same evidence-role structure — never
//! from one lucky episode (§31) and never from final-answer text (§33).

use std::collections::BTreeMap;
use std::path::Path;

use crate::derived::InvestigationEpisode;
use crate::query::identifier_terms;
use crate::symbol_exposure::SymbolExposureStore;

use super::model::{
    RecipeDefinition, RecipeFamily, Selector, SelectorStep, RECIPE_SCHEMA, RECIPE_VERSION,
};
use super::store::{RecipeError, RecipeStore};

/// Minimum episode support before a motif becomes a recipe (§31, conservative).
pub const MIN_SUPPORT: u32 = 2;

/// The evidence-role motif an episode exhibits, derived from symbol exposures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[allow(clippy::enum_variant_names)]
enum Motif {
    /// decl anchor + relation references (callers/callees inspected).
    DefinitionToRelations,
    /// decl anchor + a test-file exposure.
    DefinitionToTest,
    /// decl anchor only — not enough structure for a recipe.
    DefinitionOnly,
}

/// Classify one episode's evidence-role motif from its symbol exposures.
fn motif(ep: &InvestigationEpisode, sexp: &SymbolExposureStore) -> Motif {
    let doc = sexp.docs.get(&ep.investigation_id);
    let mut has_decl = false;
    let mut has_rel = false;
    let mut has_test = false;
    if let Some(d) = doc {
        for e in &d.exposures {
            match e.exposure_kind.as_str() {
                "declaration_occurrence" => has_decl = true,
                "reference_occurrence" | "call_occurrence" => has_rel = true,
                _ => {}
            }
            if e.path.contains("test") || e.path.contains("_test") {
                has_test = true;
            }
        }
    }
    // Test evidence also inferable from observed source paths.
    if ep
        .evidence_path_mentions
        .iter()
        .any(|p| p.contains("test") || p.contains("_test"))
    {
        has_test = true;
    }
    if has_decl && has_test {
        Motif::DefinitionToTest
    } else if has_decl && has_rel {
        Motif::DefinitionToRelations
    } else {
        Motif::DefinitionOnly
    }
}

/// Mine recipes from derived episodes + symbol exposures into `store` under
/// `scope`. Returns the number of recipes written. Deterministic and
/// incremental-safe (re-running over the same history is idempotent).
pub fn mine(
    episodes: &[InvestigationEpisode],
    sexp: &SymbolExposureStore,
    scope: &str,
    store_path: &Path,
) -> Result<usize, RecipeError> {
    // Group episodes by (motif, sorted shared query identifier terms).
    let mut groups: BTreeMap<(u8, String), Vec<String>> = BTreeMap::new();
    for ep in episodes {
        let Some(q) = &ep.query_text else { continue };
        let mut terms = identifier_terms(q);
        terms.sort();
        terms.dedup();
        if terms.is_empty() {
            continue;
        }
        let m = motif(ep, sexp);
        if m == Motif::DefinitionOnly {
            continue; // not enough structure for a recipe (§36)
        }
        let key = (m as u8, terms.join("|"));
        groups
            .entry(key)
            .or_default()
            .push(ep.investigation_id.clone());
    }
    let mut store = RecipeStore::load(store_path, scope).unwrap_or_default();
    let mut written = 0;
    for ((m, termkey), invs) in groups {
        if invs.len() < MIN_SUPPORT as usize {
            continue; // §31 minimum support
        }
        let terms: Vec<String> = termkey.split('|').map(String::from).collect();
        let (family, steps) = match m {
            1 => (
                RecipeFamily::DefinitionToTestCandidates,
                vec![
                    SelectorStep {
                        id: 0,
                        selector: Selector::ResolveAnchor,
                        after: None,
                    },
                    SelectorStep {
                        id: 1,
                        selector: Selector::TestCandidatesOf,
                        after: Some(0),
                    },
                ],
            ),
            _ => (
                RecipeFamily::DefinitionToCallers,
                vec![
                    SelectorStep {
                        id: 0,
                        selector: Selector::ResolveAnchor,
                        after: None,
                    },
                    SelectorStep {
                        id: 1,
                        selector: Selector::CallersOf,
                        after: Some(0),
                    },
                ],
            ),
        };
        let recipe_id = RecipeStore::recipe_id(scope, &format!("{:?}", family), &terms);
        let r = RecipeDefinition {
            schema: RECIPE_SCHEMA.into(),
            recipe_id,
            project_scope: scope.into(),
            version: RECIPE_VERSION,
            family,
            query_family_terms: terms,
            steps,
            support: invs.len() as u32,
            provenance: invs,
        };
        store.upsert(store_path, r)?;
        written += 1;
    }
    Ok(written)
}
