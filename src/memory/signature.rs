//! QuerySignature — a deterministic, versioned V1 query representation (§18-
//! §19). No embeddings, no Jev. Built from normalized lexical terms +
//! identifier-like tokens + known-path tokens; raw text is never the only key.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::repository::digest;

/// Extract the actual QUESTION text from a benchmark-style prompt
/// ("QUESTION:\n...\n\nINSTRUCTIONS:"). Falls back to the whole text.
pub fn question_body(query: &str) -> &str {
    if let Some(i) = query.find("QUESTION:") {
        let rest = &query[i + 9..];
        let end = rest
            .find("INSTRUCTIONS:")
            .or_else(|| rest.find("\n\n"))
            .unwrap_or(rest.len());
        return rest[..end].trim();
    }
    query.trim()
}

/// A token is "identifier-like" if it has camelCase, snake_case, or code-y
/// characters (letters+digits+`_`/`::`/`()`), distinguishing code terms from
/// prose.
fn is_identifier(tok: &str) -> bool {
    let t = tok.trim_matches(|c: char| !(c.is_alphanumeric() || c == '_' || c == ':'));
    if t.len() < 3 || !t.chars().any(|c| c.is_ascii_alphabetic()) {
        return false;
    }
    t.contains('_')
        || t.contains("::")
        || t.chars().any(|c| c.is_ascii_uppercase())
        || t.chars().any(|c| c.is_ascii_digit())
}

/// Persisted versioned query signature (§19).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QuerySignature {
    pub schema: String,
    /// sha256 of the raw query (canonical digest, §6/§18).
    pub raw_digest: String,
    /// Normalized lexical terms (lowercase, ws-normalized).
    #[serde(default)]
    pub terms: Vec<String>,
    /// Identifier-like / code tokens (camelCase, snake_case, ::, digits).
    #[serde(default)]
    pub identifiers: Vec<String>,
    /// Known repository paths present in the query.
    #[serde(default)]
    pub paths: Vec<String>,
}

/// Build a QuerySignature from raw query text. `known_paths` grounds path
/// tokens; `intent` is optional deterministic intent (e.g. find/callers).
pub fn query_signature(
    query: &str,
    known_paths: &BTreeSet<String>,
    intent: Option<&str>,
) -> QuerySignature {
    let q = question_body(query);
    let mut terms = BTreeSet::new();
    let mut identifiers = BTreeSet::new();
    let mut paths = BTreeSet::new();
    for tok in q.split(|c: char| !(c.is_alphanumeric() || "/._-:".contains(c))) {
        let t = tok.trim_matches(|c: char| !(c.is_alphanumeric() || "/._-:".contains(c)));
        if t.is_empty() {
            continue;
        }
        // path token?
        let p = t.trim_end_matches('.').trim_start_matches("./");
        if p.contains('/') && known_paths.contains(p) {
            paths.insert(p.to_string());
            continue;
        }
        let low = t.to_lowercase();
        // skip ultra-common stopwords
        if matches!(
            low.as_str(),
            "the"
                | "a"
                | "an"
                | "and"
                | "or"
                | "of"
                | "to"
                | "in"
                | "is"
                | "are"
                | "what"
                | "where"
                | "which"
                | "how"
                | "does"
                | "do"
                | "for"
                | "that"
                | "this"
                | "it"
                | "they"
                | "each"
                | "code"
                | "you"
        ) {
            continue;
        }
        terms.insert(low.clone());
        if is_identifier(t) {
            identifiers.insert(t.trim_end_matches(".go").to_string());
        }
    }
    let mut sig = QuerySignature {
        schema: "reposuite.query-signature.v1".into(),
        raw_digest: digest::sha256_text(query),
        terms: terms.into_iter().collect(),
        identifiers: identifiers.into_iter().collect(),
        paths: paths.into_iter().collect(),
    };
    let _ = intent;
    let _ = &mut sig;
    sig
}
