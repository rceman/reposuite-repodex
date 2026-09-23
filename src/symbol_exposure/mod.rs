//! SymbolExposure — derive symbol-level exposure from `SourceObserved` (§1).
//!
//! Extends the evidence chain:
//!
//! ```text
//! SourceObserved
//!     +
//! RepoDex parsed repository intelligence (FileAnalysis symbol maps)
//!     ↓
//! SymbolExposure            (symbol/occurrence source exposed to the Agent)
//!     ↓
//! symbol AgentActivity / InvestigationEpisode facts / memory symbol evidence
//! ```
//!
//! Proves only that symbol-associated source bytes/ranges were contained in
//! content exposed to the Agent — never that the model attended to, reasoned
//! about, or understood it (§1). Harness-neutral; language understanding lives
//! in RepoDex, not adapters (§3-§4). Derived and rebuildable (§27-§29).

pub mod activity;
pub mod model;
pub mod provider;
pub mod resolve;
pub mod store;

pub use activity::{SymbolActivity, SymbolActivityStore};
pub use model::{
    Certainty, ExposureKind, OverlapClass, RangePrecision, ResolutionRecord, ResolutionSource,
    ResolutionState, SymbolExposure, SYMBOL_EXPOSURE_SCHEMA_VERSION,
};
pub use provider::{MapResolution, ProviderDiag, SourceStore, SymbolMapProvider};
pub use resolve::{resolve_observation, Observation};
pub use store::{
    derive_symbol_exposures, SymbolExposureDoc, SymbolExposureReport, SymbolExposureStore,
};
