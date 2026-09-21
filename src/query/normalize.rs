//! Deterministic lexical normalization for code identifiers and query text.
//!
//! Splits identifiers on explicit separators (`_`, `-`, `/`, `.`, `::`, space,
//! ...) and on case/digit transitions, lowercasing every term. No probabilistic
//! stemming, no embeddings, no randomness — identical input always yields
//! identical terms.
//!
//! ```text
//! authToken   -> auth token
//! AuthToken   -> auth token
//! auth_token  -> auth token
//! auth-token  -> auth token
//! auth/token  -> auth token
//! HTTPRequest -> http request
//! vec2d       -> vec 2d   (letter/digit boundary)
//! ```

/// Split an identifier or free-text fragment into lowercase lexical terms.
///
/// Rules (all deterministic):
/// - non-alphanumeric characters are separators;
/// - `lower/digit -> Upper` is a boundary (`authToken`, `v2X`);
/// - `Upper -> Upper lower` ends an acronym run (`HTTPRequest` -> `HTTP`+`Request`);
/// - `letter <-> digit` is a boundary (`vec2d` -> `vec`,`2d`);
/// - every term is lowercased with full Unicode lowercasing.
pub fn identifier_terms(text: &str) -> Vec<String> {
    let mut terms = Vec::new();
    let mut cur = String::new();
    let chars: Vec<char> = text.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        if !c.is_alphanumeric() {
            flush(&mut cur, &mut terms);
            continue;
        }
        let prev = chars.get(i.wrapping_sub(1)).copied();
        let next = chars.get(i + 1).copied();
        let boundary = match prev {
            None => false,
            Some(p) => {
                let p_alpha = p.is_alphabetic();
                let c_alpha = c.is_alphabetic();
                if p_alpha != c_alpha {
                    true // letter <-> digit
                } else if p.is_lowercase() && c.is_uppercase() {
                    true // camel hump
                } else if p.is_uppercase()
                    && c.is_uppercase()
                    && next.is_some_and(|n| n.is_lowercase())
                {
                    true // acronym tail: `HTTPRequest` splits before `Request`
                } else {
                    false
                }
            }
        };
        if boundary {
            flush(&mut cur, &mut terms);
        }
        cur.push(c);
    }
    flush(&mut cur, &mut terms);
    terms
}

fn flush(cur: &mut String, terms: &mut Vec<String>) {
    if !cur.is_empty() {
        terms.push(cur.to_lowercase());
        cur.clear();
    }
}

/// Normalize a whole free-text query into its lowercase terms.
///
/// This is just `identifier_terms` over the raw text — query words and code
/// identifiers share the same token alphabet, so a single splitter is enough.
pub fn query_terms(text: &str) -> Vec<String> {
    identifier_terms(text)
}

/// Lowercase the full identifier without splitting (for exact-match keys).
pub fn lowercase(text: &str) -> String {
    text.to_lowercase()
}
