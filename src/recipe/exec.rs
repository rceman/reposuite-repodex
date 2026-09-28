//! Recipe match + current-view execution (Part B3-B4, §16-§28).
//!
//! Execution resolves the anchor in the *current* validated view and runs the
//! typed selector steps through the existing query engine — the recipe never
//! caches prior callers/tests and never produces source truth itself (§21-§23).

use serde::Serialize;

use crate::graph::model::NodeKind;
use crate::query::{QueryEngine, QueryIntent, QueryPlan, QueryResult};

use super::model::{RecipeDefinition, RecipeFamily, RecipeMatch, Selector};

/// Per-step execution record for the debug trace (§28).
#[derive(Debug, Clone, Serialize)]
pub struct StepTrace {
    pub op: String,
    pub produced: usize,
    pub skipped: bool,
    pub reason: String,
}

/// Outcome of executing one recipe against the current view.
#[derive(Debug, Clone)]
pub struct RecipeOutcome {
    pub recipe_id: String,
    pub family: String,
    pub matched: RecipeMatch,
    /// Current-view witness evidence produced (caller/callee/related hits).
    pub produced: Vec<crate::query::RelatedHit>,
    /// Steps executed/skipped + guard reasons (debug-only trace).
    pub steps: Vec<StepTrace>,
    /// Whether the recipe fell back (anchor absent/ambiguous -> no execution).
    pub fell_back: bool,
}

/// Conservative recipe match (§16-§17): the recipe's query-family terms must be
/// a subset of the query's identifier terms AND the anchor must resolve to a
/// unique declaration in the current view. Otherwise NoMatch/Ambiguous.
pub fn match_recipe<'a>(
    recipes: impl Iterator<Item = &'a RecipeDefinition>,
    query_terms: &[String],
    intent: QueryIntent,
) -> (RecipeMatch, Option<RecipeDefinition>) {
    let mut matched: Vec<&RecipeDefinition> = Vec::new();
    for r in recipes {
        // Family must be consistent with the requested intent when the intent
        // is a relationship/connector (a callers recipe can't serve a paths
        // query). `find` is open to any family.
        let ok_intent = matches!(
            (intent, r.family),
            (QueryIntent::Callers, RecipeFamily::DefinitionToCallers)
                | (QueryIntent::Callees, RecipeFamily::DefinitionToCallees)
                | (QueryIntent::Paths, RecipeFamily::TwoAnchorConnector)
                | (QueryIntent::Find, _)
        );
        if !ok_intent {
            continue;
        }
        // Term coverage: all recipe family terms present in the query terms.
        if r.query_family_terms.is_empty()
            || r.query_family_terms.iter().all(|t| query_terms.contains(t))
        {
            matched.push(r);
        }
    }
    match matched.len() {
        0 => (RecipeMatch::NoMatch, None),
        1 => (RecipeMatch::Match, Some(matched[0].clone())),
        _ => (RecipeMatch::Ambiguous, None),
    }
}

/// Execute a matched recipe against the current view. The anchor is resolved
/// fresh from the query result's top declaration seed; selector steps run the
/// existing engine ops. Membership/collections are always re-derived from the
/// current view — never replayed (§21-§22, §49).
pub fn execute(
    recipe: &RecipeDefinition,
    engine: &QueryEngine,
    anchor_result: &QueryResult,
) -> RecipeOutcome {
    let mut steps = Vec::new();
    let mut produced = Vec::new();
    // Anchor: top declaration seed of the current query result. A single-anchor
    // recipe requires exactly ONE distinct declaration anchor — pick the first
    // declaration seed, but if multiple *distinct* declaration keys match the
    // same top label, the anchor is ambiguous and we must not execute as if it
    // were unique (§8). We never choose an arbitrary first match.
    let decl_seeds: Vec<&crate::query::ScoredNode> = anchor_result
        .seeds
        .iter()
        .filter(|s| matches!(s.node.kind, NodeKind::Declaration))
        .collect();
    if decl_seeds.is_empty() {
        steps.push(StepTrace {
            op: "resolve_anchor".into(),
            produced: 0,
            skipped: true,
            reason: "anchor_absent".into(),
        });
        return RecipeOutcome {
            recipe_id: recipe.recipe_id.clone(),
            family: format!("{:?}", recipe.family),
            matched: RecipeMatch::Match,
            produced,
            steps,
            fell_back: true,
        };
    }
    // Distinct declaration keys carrying the same top label = ambiguous anchor.
    let top_label = decl_seeds[0].node.label.clone();
    let distinct_anchor_keys: std::collections::BTreeSet<&String> = decl_seeds
        .iter()
        .filter(|s| s.node.label == top_label)
        .map(|s| &s.node.key)
        .collect();
    if distinct_anchor_keys.len() > 1 {
        steps.push(StepTrace {
            op: "resolve_anchor".into(),
            produced: 0,
            skipped: true,
            reason: format!("anchor_ambiguous:{} distinct", distinct_anchor_keys.len()),
        });
        return RecipeOutcome {
            recipe_id: recipe.recipe_id.clone(),
            family: format!("{:?}", recipe.family),
            matched: RecipeMatch::Ambiguous,
            produced,
            steps,
            fell_back: true,
        };
    }
    let anchor = decl_seeds[0];
    let anchor_key = anchor.node.key.clone();
    steps.push(StepTrace {
        op: "resolve_anchor".into(),
        produced: 1,
        skipped: false,
        reason: anchor_key.clone(),
    });
    for step in &recipe.steps {
        let op = format!("{:?}", step.selector);
        // Run the selector by issuing a fresh sub-query on the anchor — the
        // engine re-derives the *current* relation set (never cached).
        let sub_intent = match step.selector {
            Selector::ResolveAnchor | Selector::ResolveMemoryAnchor { .. } => {
                steps.push(StepTrace {
                    op,
                    produced: 0,
                    skipped: true,
                    reason: "anchor_resolved".into(),
                });
                continue;
            }
            Selector::CallersOf => QueryIntent::Callers,
            Selector::CalleesOf => QueryIntent::Callees,
            Selector::RelatedOf => QueryIntent::Related,
            // Test candidates are NOT callees. Use the broader related-edge
            // domain and keep only nodes that are actually test-shaped —
            // never label an arbitrary callee a test.
            Selector::TestCandidatesOf => QueryIntent::Related,
            Selector::OwningModuleOf => QueryIntent::Related,
            Selector::PathTo { .. } => QueryIntent::Paths,
        };
        let plan = QueryPlan::parse(
            &anchor.node.label,
            crate::query::QueryMode::Ranked,
            Some(sub_intent),
            Some(anchor.node.label.clone()),
            match &step.selector {
                Selector::PathTo { to } => Some(to.clone()),
                _ => None,
            },
            64,
            None,
        );
        if plan.validate().is_err() {
            steps.push(StepTrace {
                op,
                produced: 0,
                skipped: true,
                reason: "plan_invalid".into(),
            });
            continue;
        }
        let res = engine.run(&plan);
        let test_only = matches!(step.selector, Selector::TestCandidatesOf);
        let n = res.related.len();
        for h in res.related {
            // TEST_EVIDENCE honesty (§11): only nodes that are test-shaped are
            // produced as test candidates — a plain callee is not a test.
            if test_only && !is_test_shaped(&h.node.label, &h.node.path) {
                continue;
            }
            produced.push(h);
        }
        steps.push(StepTrace {
            op,
            produced: n,
            skipped: false,
            reason: String::new(),
        });
    }
    let _ = anchor_key;
    RecipeOutcome {
        recipe_id: recipe.recipe_id.clone(),
        family: format!("{:?}", recipe.family),
        matched: RecipeMatch::Match,
        produced,
        steps,
        fell_back: false,
    }
}

/// A node is test-shaped when its label or path is recognizably a test —
/// the `TestCandidatesOf` selector must not label arbitrary callees as tests.
fn is_test_shaped(label: &str, path: &str) -> bool {
    let l = label.to_lowercase();
    let p = path.to_lowercase();
    l.starts_with("test")
        || l.contains("_test")
        || l.contains("test_")
        || l.ends_with("test")
        || p.contains("test")
        || p.contains("spec")
}
