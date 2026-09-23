//! Derived intelligence layer on top of canonical `AgentEvent v1`.
//!
//! `Agent Event Store` (raw, immutable)
//!   -> Episode Aggregator
//!      -> InvestigationEpisode  (one logical investigation, §4-§22)
//!      -> SourceExposure        (per-path content actually delivered, §11-§14)
//!      -> AgentActivity index   (accumulated per-path usage, §23-§29)
//!
//! All artifacts are REBUILDABLE from canonical events. Observational only —
//! no usefulness/ranking (that is the future Investigation Memory layer).

pub mod activity;
pub mod episode;
pub mod model;
pub mod session;
pub mod store;

pub use activity::ActivityIndex;
pub use episode::{extract_path_mentions, merge_episode};
pub use model::*;
pub use session::{derive_session, merge_part, normalize_query, SessionPart};
pub use store::{derive, verify, DeriveReport, DerivedError, DerivedStore};
