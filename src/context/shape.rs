//! Deterministic query-shape classification (Part II, §5-§7).
//!
//! Uses the existing parsed `QueryIntent` plus observable retrieval state —
//! never a model and never English-keyword guessing alone (§6). When the shape
//! cannot be determined safely the result is `Unknown`, which selects a
//! conservative bounded packet (§7).

use serde::Serialize;

use crate::query::{QueryIntent, QueryResult};

/// The bounded deterministic query-shape taxonomy (§5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryShape {
    /// One unambiguous exact declaration match.
    ExactLookup,
    /// Multiple same-name/plausible candidates needing disambiguation.
    AmbiguousLookup,
    /// Caller/callee/related-edge question.
    Relationship,
    /// Two-endpoint connector/path question.
    PathOrConnector,
    /// Broad neighborhood/orientation question.
    Orientation,
    /// Test/verification-oriented question.
    TestOrVerification,
    /// Cannot be safely classified — conservative bounded packet.
    Unknown,
}

impl QueryShape {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ExactLookup => "exact_lookup",
            Self::AmbiguousLookup => "ambiguous_lookup",
            Self::Relationship => "relationship",
            Self::PathOrConnector => "path_or_connector",
            Self::Orientation => "orientation",
            Self::TestOrVerification => "test_or_verification",
            Self::Unknown => "unknown",
        }
    }
}

/// Deterministic reason codes for the chosen shape (inspectable, §32).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ShapeReason {
    IntentPaths,
    IntentRelationship,
    SingleExactDecl,
    SameNameAmbiguity,
    TestEvidencePresent,
    BroadFind,
    Unclassified,
}

/// Classify the query deterministically from plan + retrieval result.
///
/// `exact` = a single top-ranked declaration seed whose label is an exact match
/// of a query term/identifier. `ambiguous` = >1 declaration seed sharing the
/// top label. A `paths`/`relationship` intent always wins over seed shape.
pub fn classify(plan_intent: QueryIntent, result: &QueryResult) -> (QueryShape, ShapeReason) {
    // Intent-driven shapes take precedence (§6).
    match plan_intent {
        QueryIntent::Paths => return (QueryShape::PathOrConnector, ShapeReason::IntentPaths),
        QueryIntent::Callers | QueryIntent::Callees | QueryIntent::Related => {
            return (QueryShape::Relationship, ShapeReason::IntentRelationship);
        }
        QueryIntent::Find => {}
    }
    // Seed-driven: a single exact declaration -> exact; several same-name decls
    // -> ambiguous; otherwise the broad `find` defaults to orientation.
    let decls: Vec<_> = result
        .seeds
        .iter()
        .filter(|s| matches!(s.node.kind, crate::graph::model::NodeKind::Declaration))
        .collect();
    if decls.is_empty() {
        return (QueryShape::Unknown, ShapeReason::Unclassified);
    }
    let top_label = decls[0].node.label.to_lowercase();
    let same_name = decls
        .iter()
        .filter(|s| s.node.label.to_lowercase() == top_label)
        .count();
    if same_name > 1 {
        return (QueryShape::AmbiguousLookup, ShapeReason::SameNameAmbiguity);
    }
    // A name-unique top declaration whose identifier-terms fully cover the
    // query terms is an exact lookup even when `find` also surfaces lower-ranked
    // near-name/file matches — those are dropped by the exact policy (§20).
    let terms = &result.plan.terms;
    let top_terms = crate::query::identifier_terms(&decls[0].node.label);
    let full_cover = !terms.is_empty() && terms.iter().all(|t| top_terms.contains(t));
    if full_cover || decls.len() == 1 {
        return (QueryShape::ExactLookup, ShapeReason::SingleExactDecl);
    }
    // Test-ish queries: any seed path/factor marks a test (deterministic flag).
    if result
        .seeds
        .iter()
        .any(|s| s.factors.iter().any(|f| f.contains("test")))
    {
        return (
            QueryShape::TestOrVerification,
            ShapeReason::TestEvidencePresent,
        );
    }
    (QueryShape::Orientation, ShapeReason::BroadFind)
}
