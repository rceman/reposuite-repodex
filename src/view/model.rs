//! RepositoryView model: the exact effective filesystem source tree being
//! queried, distinct from the logical `Project` (repository identity).
//!
//! A `RepositoryView` may be a normal checkout, a Git worktree, a detached or
//! dirty checkout, or a future sandbox. RepoDex never infers authority from the
//! process cwd or from a session repository — an explicit locator names the
//! exact view.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const QUERY_REQUEST_SCHEMA: &str = "reposuite.repodex.query.request.v1";
pub const QUERY_RESPONSE_SCHEMA: &str = "reposuite.repodex.query.response.v1";

/// A locator naming the exact RepositoryView to query (§4-§9).
///
/// Exactly one form is valid: `root` (explicit filesystem view) XOR
/// `project` (registered alias + optional `view`). `--view` without `--project`
/// is invalid, and both forms together are rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViewLocator {
    /// Form A — exact filesystem root.
    Root(PathBuf),
    /// Form B — registered project alias + optional view name.
    Project {
        project: String,
        view: Option<String>,
    },
}

/// A machine query request read from stdin (§30-§32).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryRequest {
    #[serde(default)]
    pub schema: Option<String>,
    #[serde(default)]
    pub root: Option<String>,
    #[serde(default)]
    pub project: Option<String>,
    #[serde(default)]
    pub view: Option<String>,
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub intent: Option<String>,
    #[serde(default)]
    pub max_results: Option<usize>,
}

/// Generic RepositoryView metadata (§19, §29). Branch is metadata only — never
/// source identity (§20); `view_fingerprint`/`head`/content digests carry the
/// actual version.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RepositoryView {
    /// Logical repository identity (registered project alias, or `root` for an
    /// explicit filesystem view).
    pub repository_id: String,
    /// External view name (registered view name, or the root basename).
    pub view_id: String,
    /// Canonical absolute filesystem root of the view.
    pub canonical_root: PathBuf,
    /// HEAD commit when the view is inside a Git checkout.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head: Option<String>,
    /// Checked-out branch when available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// True when HEAD is detached (no symbolic branch).
    pub detached: bool,
    /// True when the working tree differs from HEAD.
    pub dirty: bool,
    /// Git common-dir identity shared by linked worktrees, when detected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_common_dir: Option<PathBuf>,
    /// Content fingerprint of the view's effective source manifest
    /// (`snapshot_digest` of the current path→digest map). Authoritative
    /// content identity — never branch-derived.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view_fingerprint: Option<String>,
    /// Locator provenance (`root` or `project`).
    pub locator: String,
}

/// Deterministic machine error categories (§44).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewError {
    InvalidRequest,
    LocatorConflict,
    ProjectNotFound,
    ViewNotFound,
    ViewPathEscape,
    RootNotFound,
    NotARepository,
    QueryFailed,
    Registry,
    Io,
}

impl ViewError {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InvalidRequest => "INVALID_REQUEST",
            Self::LocatorConflict => "LOCATOR_CONFLICT",
            Self::ProjectNotFound => "PROJECT_NOT_FOUND",
            Self::ViewNotFound => "VIEW_NOT_FOUND",
            Self::ViewPathEscape => "VIEW_PATH_ESCAPE",
            Self::RootNotFound => "ROOT_NOT_FOUND",
            Self::NotARepository => "NOT_A_REPOSITORY",
            Self::QueryFailed => "QUERY_FAILED",
            Self::Registry => "REGISTRY_ERROR",
            Self::Io => "IO_ERROR",
        }
    }
    /// Suggested non-zero process exit code (§45).
    pub fn exit_code(self) -> u8 {
        match self {
            Self::InvalidRequest | Self::LocatorConflict | Self::ViewPathEscape => 2,
            Self::ProjectNotFound | Self::ViewNotFound | Self::RootNotFound => 3,
            Self::NotARepository => 4,
            _ => 1,
        }
    }
}

impl std::fmt::Display for ViewError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
impl std::error::Error for ViewError {}

pub type ViewResult<T> = Result<T, (ViewError, String)>;

/// Error tuple for use in `map_err`/`ok_or_else`.
pub fn e(kind: ViewError, msg: impl Into<String>) -> (ViewError, String) {
    (kind, msg.into())
}

pub fn err<T>(k: ViewError, msg: impl Into<String>) -> ViewResult<T> {
    Err((k, msg.into()))
}
