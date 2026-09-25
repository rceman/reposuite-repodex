//! Trajectory Evidence Utility Policy V1: instrument which evidence
//! representations reduce downstream Agent work — association, not causation.

pub mod assoc;
pub mod model;
pub mod policy;

pub use assoc::{AssocCell, UtilityStats, MIN_SUPPORT, SHRINKAGE};
pub use model::{
    ContinuationMetrics, DeliveryDecision, EvidenceRole, EvidenceUnit, Optionality, ReprKind,
    UTILITY_SCHEMA,
};
pub use policy::{decide, UtilityPolicy, UtilityTrace, MIN_MARGIN};
