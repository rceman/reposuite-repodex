//! Collision-resistant digests for repository snapshot content.
//!
//! TASK 2's canonical digest is an FNV-1a change detector. It is fine for
//! "did the fact text change", but it is not collision-resistant and must not
//! be used as authoritative source-content identity. TASK 3A therefore uses
//! SHA-256 for two distinct jobs:
//!
//! * [`content_digest`] — identity of the *source bytes* read for one file. This
//!   is the authoritative input to the reuse decision.
//! * [`analysis_digest`] — identity of the *normalized facts* produced from
//!   those bytes. Two analyses with the same digest carry the same facts.
//!
//! The algorithm is SHA-256 from the small `sha2` RustCrypto crate, which was
//! already present in the local cargo cache, so the build stays reproducible
//! offline. Source hashing is not a bottleneck: the measured corpora total
//! roughly 68 MiB, which SHA-256 digests in well under a second.

use sha2::{Digest, Sha256};

/// Algorithm label prefixed to every digest string.
pub const DIGEST_ALGORITHM: &str = "sha256";

/// SHA-256 over arbitrary bytes, rendered as `sha256:<lowercase-hex>`.
pub fn sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{DIGEST_ALGORITHM}:{:x}", hasher.finalize())
}

/// SHA-256 over a text buffer.
pub fn sha256_text(text: &str) -> String {
    sha256(text.as_bytes())
}

/// Content digest of the source bytes read for one file.
pub fn content_digest(bytes: &[u8]) -> String {
    sha256(bytes)
}

/// Analysis digest of the normalized facts of one file.
pub fn analysis_digest(analysis: &crate::model::FileAnalysis) -> String {
    sha256_text(&analysis.canonical_text())
}

/// True when a string looks like a digest this module produced.
pub fn is_digest(value: &str) -> bool {
    value
        .strip_prefix(DIGEST_ALGORITHM)
        .and_then(|rest| rest.strip_prefix(':'))
        .map(|hex| hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .unwrap_or(false)
}
