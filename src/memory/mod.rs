//! `memory` — Investigation Memory: rebuildable historical navigation priors
//! derived from InvestigationEpisodes (§16-§38). Observational/navigation aid
//! only — never promotes an entity to source truth (§2), never a query->answer
//! cache (§17), never feeds production ranking.

pub mod model;
pub mod signature;
pub mod store;

pub use model::*;
pub use signature::{query_signature, QuerySignature};
pub use store::{build_memory, MemoryError, MemoryStore};
pub mod compose;
pub mod live;
pub mod project;
pub mod rebind;
