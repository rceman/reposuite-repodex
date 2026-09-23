//! Resolve `SourceObserved` -> `SymbolExposure` records (§17-§24).
//!
//! Given the symbol map for the exact observed source version, emit one
//! exposure per overlapping declaration / reference / call. Line precision is
//! used (SourceObserved carries line ranges); byte precision is never
//! manufactured (§8).

use crate::agent_event::model::ObservationKind;
use crate::model::{Declaration, SourceRange};

use super::model::{
    Certainty, ExposureKind, OverlapClass, RangePrecision, ResolutionRecord, ResolutionState,
    SymbolExposure, SYMBOL_EXPOSURE_SCHEMA_VERSION,
};
use super::provider::SymbolMapProvider;

/// One `source_observed` observation reduced to the fields symbol resolution
/// needs. Built from an `AgentEvent` (canonical) — never harness-native.
#[derive(Debug, Clone)]
pub struct Observation {
    pub investigation_id: String,
    pub session_id: String,
    pub repository_id: String,
    pub repo_head: Option<String>,
    pub source_event_sequence: u64,
    pub path: String,
    /// 1-based inclusive line range of the delivered fragment.
    pub line_start: Option<u64>,
    pub line_end: Option<u64>,
    /// Observed whole-file content version (`sha256:`), if captured.
    pub file_content_digest: Option<String>,
    pub kind: ObservationKind,
}

fn decl_symbol_id(path: &str, declaration_id: u32) -> String {
    format!("decl:{path}#{declaration_id}")
}

fn rows(r: &SourceRange) -> (i64, i64) {
    (r.row_start as i64, r.row_end as i64)
}

/// Overlap of observed 0-based row range `[lo, hi]` with a symbol row range.
fn classify(obs_lo: i64, obs_hi: i64, sym_lo: i64, sym_hi: i64) -> Option<OverlapClass> {
    if sym_hi < obs_lo || sym_lo > obs_hi {
        return None; // no overlap
    }
    if sym_lo >= obs_lo && sym_hi <= obs_hi {
        Some(OverlapClass::SymbolInsideObserved)
    } else if obs_lo >= sym_lo && obs_hi <= sym_hi {
        Some(OverlapClass::ObservedInsideSymbol)
    } else {
        Some(OverlapClass::Partial)
    }
}

/// Resolve one observation into symbol exposures + a resolution record.
pub fn resolve_observation(
    provider: &SymbolMapProvider,
    obs: &Observation,
) -> (Vec<SymbolExposure>, ResolutionRecord) {
    let res = provider.resolve(&obs.path, obs.file_content_digest.as_deref());
    let mut rec = ResolutionRecord {
        source_event_sequence: obs.source_event_sequence,
        session_id: obs.session_id.clone(),
        investigation_id: obs.investigation_id.clone(),
        path: obs.path.clone(),
        file_content_digest: obs.file_content_digest.clone(),
        source: res.source,
        resolution: ResolutionState::UnresolvedNoSymbolOverlap,
        exposures: 0,
    };
    let analysis = match res.analysis {
        Some(a) => a,
        None => {
            rec.resolution = res
                .unresolved
                .unwrap_or(ResolutionState::UnresolvedMissingSourceVersion);
            return (Vec::new(), rec);
        }
    };
    if analysis.declarations.is_empty()
        && analysis.references.is_empty()
        && analysis.calls.is_empty()
    {
        rec.resolution = ResolutionState::UnresolvedUnsupportedLanguage;
        return (Vec::new(), rec);
    }

    // Observed 0-based row range. Whole-file when no line range was carried.
    let (obs_lo, obs_hi) = match (obs.line_start, obs.line_end) {
        (Some(a), Some(b)) => (a.saturating_sub(1) as i64, b.saturating_sub(1) as i64),
        _ => (0, i64::MAX),
    };
    let precision = match (obs.line_start, obs.line_end) {
        (Some(_), Some(_)) => RangePrecision::Line,
        _ => RangePrecision::WholeFile,
    };

    let mut out = Vec::new();
    let ctx = Ctx {
        obs,
        precision,
        digest: res.digest.as_deref(),
    };
    // enclosing candidates: decls whose range overlaps the observed window.
    let mut enclosing: Vec<(usize, &Declaration, OverlapClass)> = Vec::new();
    for decl in &analysis.declarations {
        let (dl, dh) = rows(&decl.range);
        let (nl, nh) = rows(&decl.name_range);
        // declaration_occurrence: the decl name/header row was actually shown.
        if let Some(ov) = classify(obs_lo, obs_hi, nl, nh) {
            out.push(ctx.decl_exposure(decl, ov, ExposureKind::DeclarationOccurrence));
        } else if let Some(ov) = classify(obs_lo, obs_hi, dl, dh) {
            // observed inside decl (or partial) -> enclosing_declaration
            enclosing.push((decl.range.byte_len() as usize, decl, ov));
        }
    }
    // nested: sort enclosing by range size ascending -> innermost first (§23).
    enclosing.sort_by_key(|(sz, d, _)| (*sz, d.declaration_id));
    for (depth, (_sz, decl, ov)) in enclosing.into_iter().enumerate() {
        out.push(
            ctx.decl_exposure(decl, ov, ExposureKind::EnclosingDeclaration)
                .with_depth(depth as u32, depth == 0),
        );
    }

    // reference_occurrence: a resolved/bounded reference occurrence in-window.
    for r in &analysis.references {
        let (rl, rh) = rows(&r.range);
        if classify(obs_lo, obs_hi, rl, rh).is_none() {
            continue;
        }
        let (target, cert, sym) = match r.declaration_id {
            Some(id) => (
                decl_symbol_id(&obs.path, id),
                Certainty::Fact,
                decl_symbol_id(&obs.path, id),
            ),
            None => (r.written.clone(), Certainty::Candidate, r.written.clone()),
        };
        let mut e = ctx.occ_exposure(&sym, &r.written, "reference", cert);
        e.reference_target = Some(target);
        e.symbol_line_start = r.range.row_start;
        e.symbol_line_end = r.range.row_end;
        out.push(e);
    }
    for c in &analysis.calls {
        let (cl, ch) = rows(&c.callee_range);
        if classify(obs_lo, obs_hi, cl, ch).is_none() {
            continue;
        }
        // Call-like callee stays a bounded candidate unless the enclosing decl
        // resolves it structurally (kept CANDIDATE — §21/§22).
        let mut e = ctx.occ_exposure(
            &c.callee_written,
            &c.callee_written,
            "call",
            Certainty::Candidate,
        );
        e.reference_target = Some(c.callee_written.clone());
        e.symbol_line_start = c.callee_range.row_start;
        e.symbol_line_end = c.callee_range.row_end;
        out.push(e);
    }

    rec.exposures = out.len();
    rec.resolution = if out.is_empty() {
        ResolutionState::UnresolvedNoSymbolOverlap
    } else if !analysis.recovery_regions.is_empty() {
        ResolutionState::Partial
    } else {
        ResolutionState::Resolved
    };
    (out, rec)
}

/// Per-observation context shared by every emitted exposure.
struct Ctx<'a> {
    obs: &'a Observation,
    precision: RangePrecision,
    digest: Option<&'a str>,
}

impl Ctx<'_> {
    /// Exposure for a declaration symbol (enclosing or declaration occurrence).
    fn decl_exposure(
        &self,
        decl: &Declaration,
        overlap: OverlapClass,
        kind: ExposureKind,
    ) -> SymbolExposure {
        let mut e = self.base(
            &decl_symbol_id(&self.obs.path, decl.declaration_id),
            &decl.name,
            decl.kind.as_str(),
            overlap,
            kind,
            Certainty::Fact,
        );
        e.symbol_line_start = decl.range.row_start;
        e.symbol_line_end = decl.range.row_end;
        e
    }

    /// Exposure for a reference/call occurrence inside the observed window.
    fn occ_exposure(
        &self,
        symbol_id: &str,
        name: &str,
        kind_tag: &str,
        cert: Certainty,
    ) -> SymbolExposure {
        self.base(
            symbol_id,
            name,
            kind_tag,
            OverlapClass::OccurrenceInsideObserved,
            ExposureKind::ReferenceOccurrence,
            cert,
        )
    }

    fn base(
        &self,
        symbol_id: &str,
        symbol_name: &str,
        symbol_kind: &str,
        overlap: OverlapClass,
        kind: ExposureKind,
        cert: Certainty,
    ) -> SymbolExposure {
        let obs = self.obs;
        SymbolExposure {
            exposure_id: format!(
                "sexp-{}-{}-{}",
                obs.session_id, obs.source_event_sequence, symbol_id
            ),
            session_id: obs.session_id.clone(),
            source_event_sequence: obs.source_event_sequence,
            path: obs.path.clone(),
            file_content_digest: self.digest.map(|d| d.to_string()),
            symbol_id: symbol_id.to_string(),
            symbol_name: symbol_name.to_string(),
            symbol_kind: symbol_kind.to_string(),
            observed_line_start: obs.line_start,
            observed_line_end: obs.line_end,
            symbol_line_start: 0,
            symbol_line_end: 0,
            overlap,
            exposure_kind: kind,
            observation_provenance: obs.kind,
            certainty: cert,
            reference_target: None,
            depth: 0,
            innermost: false,
            resolution: ResolutionState::Resolved,
            precision: self.precision,
            schema_version: SYMBOL_EXPOSURE_SCHEMA_VERSION,
        }
    }
}

impl SymbolExposure {
    fn with_depth(mut self, depth: u32, innermost: bool) -> Self {
        self.depth = depth;
        self.innermost = innermost;
        self
    }
}
