//! RepositoryView: the exact effective filesystem source tree being queried,
//! independent of process cwd, session repository, or branch (Part II-X).
//!
//! ```text
//! locator (--root | --project [--view])
//!     -> canonical RepositoryView (git metadata)
//!     -> effective manifest (path -> content digest)
//!     -> content-addressed FileAnalysis reuse
//!     -> digest-keyed derived index (snapshot -> links -> candidates -> graph)
//!     -> query
//! ```

pub mod git;
pub mod index;
pub mod manifest;
pub mod model;
pub mod registry;
pub mod resolve;
pub mod service;
pub mod validity;

pub use index::{ensure_index, EnsureOutcome};
pub use model::{
    err, QueryRequest, RepositoryView, ViewError, ViewLocator, ViewResult, QUERY_REQUEST_SCHEMA,
    QUERY_RESPONSE_SCHEMA,
};
pub use registry::{normalize_path, state_dir, Project, ProjectRegistry};
pub use resolve::{locator_from_flags, resolve};
pub use service::{run_view_query, ViewQueryOutcome, ViewQueryParams};
