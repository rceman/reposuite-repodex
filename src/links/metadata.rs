//! Minimal repository metadata reading.
//!
//! TASK 3B consumes a TASK 3A snapshot plus **only** the metadata a required
//! rule actually needs. Exactly one metadata format is implemented: the
//! `module` directive of a repository-root `go.mod`, because
//! `go.import.local_module` cannot map an import path onto a repository
//! directory without it.
//!
//! `Cargo.toml`, `composer.json` and `pyproject.toml` are deliberately **not**
//! parsed: no required TASK 3B rule needs them, and this task does not build a
//! generic configuration-file framework.
//!
//! A metadata file is identified by its repository-relative path and its
//! content digest, never by mtime, so a derived link artifact becomes invalid
//! when the metadata's content changes.

use std::path::Path;

use crate::repository::digest;

/// Relative path of the only metadata file this task reads.
pub const GO_MOD: &str = "go.mod";

/// Upper bound on a metadata file we will read. A `go.mod` is a few hundred
/// bytes; anything larger is not one.
const MAX_METADATA_BYTES: u64 = 1 << 20;

/// A repository-root `go.mod` with a usable module directive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoModule {
    pub relative_path: String,
    pub content_digest: String,
    /// The value of the `module` directive.
    pub module_path: String,
}

/// What the repository-root `go.mod` turned out to be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GoModuleState {
    /// No `go.mod` at the repository root.
    Absent,
    /// A `go.mod` exists but its module directive could not be read.
    Malformed {
        relative_path: String,
        content_digest: String,
        reason: String,
    },
    /// A `go.mod` with a module directive.
    Present(GoModule),
}

impl GoModuleState {
    /// The module path, when one was read.
    pub fn module_path(&self) -> Option<&str> {
        match self {
            GoModuleState::Present(module) => Some(&module.module_path),
            _ => None,
        }
    }

    /// The metadata dependency to record.
    ///
    /// The absence of a `go.mod` is recorded too, as a dependency on that
    /// absence, so that adding one later invalidates a derived artifact exactly
    /// as changing one does.
    pub fn dependency(&self, rule_id: &str) -> super::model::MetadataDependency {
        match self {
            GoModuleState::Absent => super::model::MetadataDependency {
                relative_path: GO_MOD.to_string(),
                content_digest: digest::sha256(&[]),
                present: false,
                rule_id: rule_id.to_string(),
                field: "module".to_string(),
                value: "absent".to_string(),
            },
            GoModuleState::Malformed {
                content_digest,
                reason,
                ..
            } => super::model::MetadataDependency {
                relative_path: GO_MOD.to_string(),
                content_digest: content_digest.clone(),
                present: true,
                rule_id: rule_id.to_string(),
                field: "module".to_string(),
                value: format!("malformed: {reason}"),
            },
            GoModuleState::Present(module) => super::model::MetadataDependency {
                relative_path: GO_MOD.to_string(),
                content_digest: module.content_digest.clone(),
                present: true,
                rule_id: rule_id.to_string(),
                field: "module".to_string(),
                value: module.module_path.clone(),
            },
        }
    }
}

/// Read and parse the repository-root `go.mod`.
///
/// Missing is not an error — it is a distinct, recorded state. Malformed is also
/// recorded rather than guessed at.
pub fn read_go_module(root: &Path) -> GoModuleState {
    let path = root.join(GO_MOD);
    let metadata = match std::fs::metadata(&path) {
        Ok(metadata) => metadata,
        Err(_) => return GoModuleState::Absent,
    };
    if metadata.len() > MAX_METADATA_BYTES {
        return GoModuleState::Malformed {
            relative_path: GO_MOD.to_string(),
            content_digest: String::new(),
            reason: format!(
                "go.mod is {} bytes, larger than the {MAX_METADATA_BYTES}-byte limit",
                metadata.len()
            ),
        };
    }
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) => {
            return GoModuleState::Malformed {
                relative_path: GO_MOD.to_string(),
                content_digest: String::new(),
                reason: format!("go.mod could not be read: {error}"),
            }
        }
    };
    let content_digest = digest::content_digest(&bytes);
    let text = match std::str::from_utf8(&bytes) {
        Ok(text) => text,
        Err(_) => {
            return GoModuleState::Malformed {
                relative_path: GO_MOD.to_string(),
                content_digest,
                reason: "go.mod is not valid UTF-8".to_string(),
            }
        }
    };
    match parse_module_directive(text) {
        Some(module_path) => GoModuleState::Present(GoModule {
            relative_path: GO_MOD.to_string(),
            content_digest,
            module_path,
        }),
        None => GoModuleState::Malformed {
            relative_path: GO_MOD.to_string(),
            content_digest,
            reason: "go.mod has no readable `module` directive".to_string(),
        },
    }
}

/// Extract the value of the `module` directive.
///
/// This understands only what the directive needs: line comments, an optional
/// quoted path, and the first whitespace-delimited token otherwise.
pub fn parse_module_directive(text: &str) -> Option<String> {
    for raw in text.lines() {
        let line = match raw.find("//") {
            Some(index) => &raw[..index],
            None => raw,
        };
        let line = line.trim();
        let Some(rest) = line.strip_prefix("module") else {
            continue;
        };
        // `modulefoo` is not the directive.
        if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
            continue;
        }
        let rest = rest.trim();
        if rest.is_empty() {
            return None;
        }
        let value = if let Some(inner) = rest.strip_prefix('"') {
            match inner.find('"') {
                Some(end) => &inner[..end],
                None => return None,
            }
        } else {
            rest.split_whitespace().next().unwrap_or("")
        };
        if value.is_empty() {
            return None;
        }
        return Some(value.to_string());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_directive_is_read_from_a_plain_go_mod() {
        let text = "// a comment\n\nmodule example.com/project\n\ngo 1.21\n";
        assert_eq!(
            parse_module_directive(text).as_deref(),
            Some("example.com/project")
        );
    }

    #[test]
    fn module_directive_handles_a_quoted_path_and_inline_comment() {
        assert_eq!(
            parse_module_directive("module \"example.com/q\" // why not").as_deref(),
            Some("example.com/q")
        );
        assert_eq!(
            parse_module_directive("module example.com/p // comment").as_deref(),
            Some("example.com/p")
        );
    }

    #[test]
    fn a_missing_directive_is_reported_not_guessed() {
        assert_eq!(parse_module_directive("go 1.21\nrequire x v1\n"), None);
        assert_eq!(parse_module_directive("modulefoo bar\n"), None);
        assert_eq!(parse_module_directive("module\n"), None);
        assert_eq!(parse_module_directive("module \"unterminated\n"), None);
    }
}
