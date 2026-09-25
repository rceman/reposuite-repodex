//! Witness materialization (§7-§10): read the CURRENT file bytes, verify the
//! digest matches the index snapshot that produced the range, then slice the
//! bounded byte span. Digest/bytes are always from the same capture — never a
//! mismatched historical range on new bytes.

use std::path::Path;

use crate::query::projection::EvidenceProjection;

use super::model::{CurrentSourceWitness, WitnessRole, WITNESS_SCHEMA};

/// Deterministic V1 bounds (§10).
pub const MAX_WITNESSES: usize = 4;
pub const MAX_WITNESS_BYTES: usize = 2048;
pub const MAX_TOTAL_WITNESS_BYTES: usize = 8192;

/// Emitted result: witnesses + source-exposure accounting (§11).
#[derive(Debug, Default)]
pub struct WitnessPacket {
    pub witnesses: Vec<CurrentSourceWitness>,
    /// Actual source bytes delivered (source exposure — counted).
    pub witness_bytes: usize,
    /// Ranges that had no witness (no safe range / digest mismatch / budget).
    pub unavailable: Vec<String>,
}

fn file_digest(bytes: &[u8]) -> String {
    // Same digest the index snapshot recorded (SourceFile.snapshot_id = snap-*).
    crate::model::snapshot_id(bytes)
}

/// Materialize witnesses for the top declaration seeds of `proj` whose
/// current declaration/body ranges exist. Reads the live file at
/// `view_root/path` and slices `byte_start..byte_end`. Emits nothing for a
/// seed when the current file digest does not equal the snapshot the range
/// came from (stale — never emit mismatched bytes, §7).
pub fn materialize(
    proj: &EvidenceProjection,
    view_root: &Path,
    index_dir: Option<&Path>,
) -> WitnessPacket {
    let mut out = WitnessPacket::default();
    let mut total = 0usize;
    // The index snapshot dir holds per-file FileAnalysis with the recorded
    // source digest (SourceFile.snapshot_id) the ranges were computed on.
    let resolver = index_dir.and_then(crate::query::projection::RangeResolver::open);
    for seed in proj.seeds.iter().take(6) {
        if out.witnesses.len() >= MAX_WITNESSES || total >= MAX_TOTAL_WITNESS_BYTES {
            out.unavailable.push(seed.key.clone());
            continue;
        }
        // Only declaration seeds with a current decl/body range can be witnessed.
        let path = &seed.path;
        let Ok(bytes) = std::fs::read(view_root.join(path)) else {
            out.unavailable.push(seed.key.clone());
            continue;
        };
        // The current file digest must equal the snapshot's digest for the
        // range to be current — otherwise the range may be stale; no witness.
        let cur = file_digest(&bytes);
        let snapshot_ok = resolver
            .as_ref()
            .and_then(|r| r.file_digest(path))
            .map(|d| d == cur)
            .unwrap_or(false);
        if !snapshot_ok {
            out.unavailable.push(seed.key.clone());
            continue;
        }
        // Prefer the body range (fullest witness) else the declaration range.
        for (range, role) in [
            (seed.body_range.as_ref(), WitnessRole::Body),
            (seed.declaration_range.as_ref(), WitnessRole::Declaration),
        ] {
            let Some(r) = range else { continue };
            let (s, e) = (
                r.byte_start as usize,
                (r.byte_end as usize).min(bytes.len()),
            );
            if s >= e || s >= bytes.len() {
                continue;
            }
            let span = &bytes[s..e];
            let (src, truncated) = if span.len() > MAX_WITNESS_BYTES {
                (
                    String::from_utf8_lossy(&span[..MAX_WITNESS_BYTES]).into_owned(),
                    true,
                )
            } else {
                (String::from_utf8_lossy(span).into_owned(), false)
            };
            let delivered = src.len();
            if total + delivered > MAX_TOTAL_WITNESS_BYTES {
                out.unavailable.push(seed.key.clone());
                break;
            }
            total += delivered;
            let complete = !truncated && e - s == span.len();
            out.witnesses.push(CurrentSourceWitness {
                schema: WITNESS_SCHEMA.into(),
                scope: proj.query.clone(),
                repository_view: "current".into(),
                repo_relative_path: path.clone(),
                role,
                evidence_class: "fact".into(),
                entity: seed.key.clone(),
                current_file_digest: cur.clone(),
                byte_start: r.byte_start,
                byte_end: r.byte_end,
                line_start: r.start_line,
                line_end: r.end_line,
                source: src,
                complete_for_range: complete,
                truncated,
                provenance: "index.snapshot.file_analysis".into(),
            });
            break; // one witness per seed
        }
        if out.witnesses.last().map(|w| w.entity.as_str()) != Some(seed.key.as_str()) {
            out.unavailable.push(seed.key.clone());
        }
    }
    out.witness_bytes = total;
    out
}
