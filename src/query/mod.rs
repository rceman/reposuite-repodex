//! The deterministic textual query engine (Phase B) and shared lexical
//! normalization (Phase A). Deterministic, model-free, graph-driven.

pub mod adaptive;
pub mod engine;
pub mod normalize;
pub mod observation;
pub mod projection;

pub use engine::{
    QueryEngine, QueryIntent, QueryMode, QueryPlan, QueryResult, RelatedHit, ScoredNode,
};
pub use normalize::{identifier_terms, query_terms};
