//! Shared bounded PHP class-name resolution.
//!
//! One deterministic implementation of PHP name semantics used by the link
//! rules (extends/implements/trait composition) AND the call-candidate rules
//! (construction, static calls, receiver types) so the same written name can
//! never resolve differently by accident.
//!
//! PHP semantics applied (per PHP): class-like names are case-insensitive —
//! callers lowercase lookup keys. `\A\B` is fully qualified, `namespace\X` is
//! current-namespace relative, `A\B` substitutes its first segment through a
//! class/namespace `use` alias else prefixes the current namespace, and
//! unqualified `X` resolves through `use` aliases else `ns\X` — classes have
//! NO global fallback (unlike functions).

use std::collections::HashMap;

use crate::model::{FileAnalysis, ImportCategory, ScopeKind};

/// Last `\`-separated segment.
pub fn last_segment(qualified: &str) -> &str {
    qualified.rsplit('\\').next().unwrap_or(qualified)
}

/// Class/namespace import alias table for one file: local name (lowercase)
/// -> written qualified target. `use function`/`use const` do not feed the
/// class-name space.
pub fn name_aliases(a: &FileAnalysis) -> HashMap<String, String> {
    let mut name_aliases: HashMap<String, String> = HashMap::new();
    for import in &a.imports {
        for item in &import.items {
            if item.category != ImportCategory::Normal {
                continue;
            }
            let written = match &import.module {
                Some(module) => format!("{module}\\{}", item.target),
                None => item.target.clone(),
            };
            let local = item
                .alias
                .clone()
                .unwrap_or_else(|| last_segment(&written).to_string());
            name_aliases.insert(local.to_lowercase(), written);
        }
    }
    name_aliases
}

/// `use function` alias table: local name (lowercase) -> written qualified
/// target.
pub fn function_aliases(a: &FileAnalysis) -> HashMap<String, String> {
    let mut function_aliases: HashMap<String, String> = HashMap::new();
    for import in &a.imports {
        for item in &import.items {
            if item.category != ImportCategory::Function {
                continue;
            }
            let written = match &import.module {
                Some(module) => format!("{module}\\{}", item.target),
                None => item.target.clone(),
            };
            let local = item
                .alias
                .clone()
                .unwrap_or_else(|| last_segment(&written).to_string());
            function_aliases.insert(local.to_lowercase(), written);
        }
    }
    function_aliases
}

/// The written namespace enclosing `scope_id` (may be multi-segment `A\B`).
pub fn enclosing_namespace(a: &FileAnalysis, scope_id: u32) -> Option<String> {
    let mut parts = Vec::new();
    for id in a.scope_chain(scope_id) {
        let s = &a.scopes[id as usize];
        if s.kind == ScopeKind::Namespace {
            if let Some(n) = &s.name {
                parts.push(n.clone());
            }
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\\"))
    }
}

/// Resolve a written PHP class-like name to its syntactic FQN under PHP
/// scoping rules. This is a *name-space* answer only — whether any
/// declaration exists is the caller's index concern.
pub fn class_fqn(
    written: &str,
    namespace: Option<&str>,
    name_aliases: &HashMap<String, String>,
) -> String {
    if let Some(rest) = written.strip_prefix('\\') {
        return rest.to_string();
    }
    if let Some(rest) = written.strip_prefix("namespace\\") {
        return match namespace {
            Some(ns) if !ns.is_empty() => format!("{ns}\\{rest}"),
            _ => rest.to_string(),
        };
    }
    let first = written.split('\\').next().unwrap_or(written);
    match name_aliases.get(&first.to_lowercase()) {
        Some(prefix) => format!("{prefix}{}", &written[first.len()..]),
        None => match namespace {
            Some(ns) if !ns.is_empty() => format!("{ns}\\{written}"),
            _ => written.to_string(),
        },
    }
}
