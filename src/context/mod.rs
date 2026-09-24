//! Deterministic adaptive Context Compiler.
//!
//! Classifies query shape, builds an evidence-obligation ledger, and selects
//! the smallest faithful evidence representation within a serialized-byte
//! budget — never weakening protocol correctness or source truth.

pub mod compiler;
pub mod shape;

pub use compiler::{
    compile, compile_into, CompiledPacket, ContextBudget, ContextTrace, Obligation,
    ObligationEntry, ObligationState, ReasonCode,
};
pub use shape::{classify, QueryShape, ShapeReason};

/// Compiler policy for a query (§37). `static` = the canonical faithful
/// projection; `adaptive` = shape-driven minimal compilation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextPolicy {
    Static,
    Adaptive,
}

impl ContextPolicy {
    pub fn parse(s: &str) -> Self {
        match s {
            "adaptive" => Self::Adaptive,
            _ => Self::Static,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Static => "static",
            Self::Adaptive => "adaptive",
        }
    }
}
