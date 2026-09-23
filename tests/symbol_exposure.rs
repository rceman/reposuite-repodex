//! SymbolExposure contract tests (§64-§74).
//!
//! Prove symbol-level exposure binds to the exact observed source version and
//! classifies enclosing/declaration/reference occurrence correctly.

mod support;

use repodex::agent_event::model::AgentEvent;
use repodex::agent_event::model::ObservationKind;
use repodex::parser::{Analyzer, AnalyzerConfig};
use repodex::repository::{artifact, build_snapshot, digest, BuildOptions};
use repodex::symbol_exposure::{
    derive_symbol_exposures, Certainty, ExposureKind, Observation, ResolutionSource,
    ResolutionState, SourceStore, SymbolExposureStore, SymbolMapProvider,
};
use support::TempDir;

const GO_A: &str = r#"package svc

func Helper() int {
	return 1
}

func Outer() int {
	x := Helper()
	return x + 1
}
"#;

fn obs(path: &str, lo: u64, hi: u64, digest: Option<String>, seq: u64) -> Observation {
    Observation {
        investigation_id: "inv".to_string(),
        session_id: "s".to_string(),
        repository_id: "r".to_string(),
        repo_head: Some("h".to_string()),
        source_event_sequence: seq,
        path: path.to_string(),
        line_start: Some(lo),
        line_end: Some(hi),
        file_content_digest: digest,
        kind: ObservationKind::ExplicitRead,
    }
}

fn snap(dir: &TempDir, files: &[(&str, &str)]) -> std::path::PathBuf {
    for (p, c) in files {
        dir.write(p, c.as_bytes());
    }
    let snap = dir.path().join("snap");
    let analyzer = Analyzer::new(AnalyzerConfig::default()).unwrap();
    build_snapshot(&analyzer, dir.path(), &snap, BuildOptions::default()).unwrap();
    snap
}

fn digest_of(dir: &TempDir, rel: &str) -> String {
    let bytes = std::fs::read(dir.path().join(rel)).unwrap();
    digest::content_digest(&bytes)
}

/// §64: observed digest == indexed digest -> indexed hit, correct exposure, no parse.
#[test]
fn indexed_digest_hit_reuses_symbol_map() {
    let dir = TempDir::new("sexp-hit");
    let snap = snap(&dir, &[("svc.go", GO_A)]);
    let manifest = artifact::read_manifest(&snap).unwrap();
    let sources = SourceStore::default();
    let provider = SymbolMapProvider::new(&snap, &manifest, &sources);
    let d = digest_of(&dir, "svc.go");
    // observe lines inside Outer body (lines 8-10: x := Helper(); return)
    let (exps, rec) = repodex::symbol_exposure::resolve_observation(
        &provider,
        &obs("svc.go", 8, 10, Some(d.clone()), 5),
    );
    assert_eq!(rec.source, ResolutionSource::IndexedHit);
    assert!(
        provider.diag.borrow().new_parse == 0,
        "no reparse on indexed hit"
    );
    // Outer encloses; Helper referenced via call at line 9.
    let outer = exps.iter().find(|e| e.symbol_name == "Outer").unwrap();
    assert_eq!(outer.exposure_kind, ExposureKind::EnclosingDeclaration);
    assert!(outer.innermost);
    let helper_ref = exps
        .iter()
        .find(|e| e.exposure_kind == ExposureKind::ReferenceOccurrence && e.symbol_name == "Helper")
        .expect("call to Helper exposed");
    assert_eq!(helper_ref.certainty, Certainty::Candidate);
}

/// §70: observing the declaration line -> declaration_occurrence.
#[test]
fn declaration_line_is_declaration_occurrence() {
    let dir = TempDir::new("sexp-decl");
    let snap = snap(&dir, &[("svc.go", GO_A)]);
    let manifest = artifact::read_manifest(&snap).unwrap();
    let sources = SourceStore::default();
    let provider = SymbolMapProvider::new(&snap, &manifest, &sources);
    let d = digest_of(&dir, "svc.go");
    // line 7: `func Outer() int {`
    let (exps, _) =
        repodex::symbol_exposure::resolve_observation(&provider, &obs("svc.go", 7, 7, Some(d), 1));
    let e = exps
        .iter()
        .find(|e| {
            e.symbol_name == "Outer" && e.exposure_kind == ExposureKind::DeclarationOccurrence
        })
        .expect("Outer declaration occurrence");
    assert_eq!(e.symbol_kind, "function");
}

/// §69: middle lines without the decl line -> enclosing_declaration (not declaration).
#[test]
fn middle_lines_are_enclosing_not_declaration() {
    let dir = TempDir::new("sexp-encl");
    let snap = snap(&dir, &[("svc.go", GO_A)]);
    let manifest = artifact::read_manifest(&snap).unwrap();
    let sources = SourceStore::default();
    let provider = SymbolMapProvider::new(&snap, &manifest, &sources);
    let d = digest_of(&dir, "svc.go");
    // line 4: `return 1` inside Helper (decl line 3 not shown)
    let (exps, _) =
        repodex::symbol_exposure::resolve_observation(&provider, &obs("svc.go", 4, 4, Some(d), 1));
    let e = exps.iter().find(|e| e.symbol_name == "Helper").unwrap();
    assert_eq!(e.exposure_kind, ExposureKind::EnclosingDeclaration);
    assert!(exps
        .iter()
        .all(|x| !(x.symbol_name == "Helper"
            && x.exposure_kind == ExposureKind::DeclarationOccurrence)));
}

/// §65/§66: a changed version resolves against its own parse, not the index.
#[test]
fn changed_version_uses_its_own_map_and_caches() {
    let dir = TempDir::new("sexp-chg");
    let snap = snap(&dir, &[("svc.go", GO_A)]);
    let manifest = artifact::read_manifest(&snap).unwrap();
    let mut sources = SourceStore::default();
    // version B: insert a line before Helper so its lines shift.
    let b = "package svc\n\n// extra comment line\nfunc Helper() int {\n\treturn 1\n}\n\nfunc Outer() int {\n\tx := Helper()\n\treturn x + 1\n}\n";
    let db = sources.put(b.as_bytes().to_vec());
    let provider = SymbolMapProvider::new(&snap, &manifest, &sources);
    // B observed: Helper body now at lines 5, not 4. Observe line 5.
    let (exps, rec) = repodex::symbol_exposure::resolve_observation(
        &provider,
        &obs("svc.go", 5, 5, Some(db.clone()), 1),
    );
    assert_eq!(rec.source, ResolutionSource::NewParse);
    let helper = exps.iter().find(|e| e.symbol_name == "Helper").unwrap();
    assert_eq!(helper.exposure_kind, ExposureKind::EnclosingDeclaration);
    assert_eq!(helper.file_content_digest.as_deref(), Some(db.as_str()));
    // repeated observation of B -> cached, no second parse.
    let (_e2, rec2) = repodex::symbol_exposure::resolve_observation(
        &provider,
        &obs("svc.go", 5, 5, Some(db.clone()), 2),
    );
    assert_eq!(rec2.source, ResolutionSource::CachedVersion);
    assert_eq!(
        provider.diag.borrow().new_parse,
        1,
        "parsed once, cached after"
    );
}

/// §68: digest differs and no source bytes -> unresolved_missing_source_version.
#[test]
fn missing_version_is_unresolved_not_guessed() {
    let dir = TempDir::new("sexp-miss");
    let snap = snap(&dir, &[("svc.go", GO_A)]);
    let manifest = artifact::read_manifest(&snap).unwrap();
    let sources = SourceStore::default(); // empty
    let provider = SymbolMapProvider::new(&snap, &manifest, &sources);
    let (exps, rec) = repodex::symbol_exposure::resolve_observation(
        &provider,
        &obs("svc.go", 4, 4, Some("sha256:deadbeef".into()), 1),
    );
    assert!(exps.is_empty());
    assert_eq!(
        rec.resolution,
        ResolutionState::UnresolvedMissingSourceVersion
    );
    assert_eq!(provider.diag.borrow().indexed_hit, 0);
}

/// §67: rapid A/B/C/D versions each bind to their own digest.
#[test]
fn rapid_edits_bind_correct_versions() {
    let dir = TempDir::new("sexp-rapid");
    let snap = snap(&dir, &[("svc.go", GO_A)]);
    let manifest = artifact::read_manifest(&snap).unwrap();
    let mut sources = SourceStore::default();
    let mut provider_digests = Vec::new();
    for i in 0..4u32 {
        let v = format!(
            "package svc\n\n// v{i}\nfunc Helper() int {{\n\treturn {}\n}}\n",
            i
        );
        provider_digests.push(sources.put(v.as_bytes().to_vec()));
    }
    let provider = SymbolMapProvider::new(&snap, &manifest, &sources);
    for (i, d) in provider_digests.iter().enumerate() {
        let (exps, rec) = repodex::symbol_exposure::resolve_observation(
            &provider,
            &obs("svc.go", 5, 5, Some(d.clone()), i as u64),
        );
        assert_eq!(rec.source, ResolutionSource::NewParse);
        let h = exps.iter().find(|e| e.symbol_name == "Helper").unwrap();
        assert_eq!(
            h.file_content_digest.as_deref(),
            Some(d.as_str()),
            "version {i} bound"
        );
    }
    assert_eq!(provider.diag.borrow().new_parse, 4);
}

/// §72: nested declarations -> multiple enclosing + innermost designated.
#[test]
fn nested_declarations_report_innermost() {
    let rs = "mod m {\n    fn outer() {\n        fn inner() {\n            let x = 1;\n        }\n    }\n}\n";
    let dir = TempDir::new("sexp-nest");
    let snap = snap(&dir, &[("m.rs", rs)]);
    let manifest = artifact::read_manifest(&snap).unwrap();
    let sources = SourceStore::default();
    let provider = SymbolMapProvider::new(&snap, &manifest, &sources);
    let d = digest_of(&dir, "m.rs");
    // line 4 `let x = 1;` is inside inner (3..5) inside outer (2..6) inside mod (1..7)
    let (exps, _) =
        repodex::symbol_exposure::resolve_observation(&provider, &obs("m.rs", 4, 4, Some(d), 1));
    let enc: Vec<_> = exps
        .iter()
        .filter(|e| e.exposure_kind == ExposureKind::EnclosingDeclaration)
        .collect();
    assert!(enc.len() >= 2, "nested enclosing recorded");
    let innermost = enc.iter().find(|e| e.innermost).unwrap();
    assert_eq!(innermost.symbol_name, "inner");
    assert_eq!(innermost.depth, 0);
}

/// §74: a search-snippet observation exposes only overlapping symbols.
#[test]
fn snippet_exposes_only_overlapping_symbols() {
    let dir = TempDir::new("sexp-snip");
    let snap = snap(&dir, &[("svc.go", GO_A)]);
    let manifest = artifact::read_manifest(&snap).unwrap();
    let sources = SourceStore::default();
    let provider = SymbolMapProvider::new(&snap, &manifest, &sources);
    let d = digest_of(&dir, "svc.go");
    let mut o = obs("svc.go", 8, 8, Some(d), 1);
    o.kind = ObservationKind::SearchSnippet;
    let (exps, _) = repodex::symbol_exposure::resolve_observation(&provider, &o);
    // line 9 `x := Helper()` -> Helper reference (candidate) + Outer enclosing.
    assert!(exps
        .iter()
        .all(|e| e.observation_provenance == ObservationKind::SearchSnippet));
    assert!(
        exps.iter()
            .any(|e| e.symbol_name == "Helper"
                && e.exposure_kind == ExposureKind::ReferenceOccurrence)
    );
    // Helper's own declaration (lines 3-5) is NOT in the observed line.
    assert!(exps
        .iter()
        .all(|e| !(e.symbol_name == "Helper"
            && e.exposure_kind == ExposureKind::DeclarationOccurrence)));
}

/// §46: no digest -> assumed indexed (read-only corpus), still resolved.
#[test]
fn missing_digest_resolves_assumed_indexed() {
    let dir = TempDir::new("sexp-nodigest");
    let snap = snap(&dir, &[("svc.go", GO_A)]);
    let manifest = artifact::read_manifest(&snap).unwrap();
    let sources = SourceStore::default();
    let provider = SymbolMapProvider::new(&snap, &manifest, &sources);
    let (exps, rec) =
        repodex::symbol_exposure::resolve_observation(&provider, &obs("svc.go", 8, 10, None, 1));
    assert_eq!(rec.source, ResolutionSource::AssumedIndexed);
    assert!(exps.iter().any(|e| e.symbol_name == "Outer"));
}

/// derive driver folds source_observed events into per-investigation docs.
#[test]
fn derive_groups_by_investigation() {
    let dir = TempDir::new("sexp-derive");
    let snap = snap(&dir, &[("svc.go", GO_A)]);
    let manifest = artifact::read_manifest(&snap).unwrap();
    let sources = SourceStore::default();
    let provider = SymbolMapProvider::new(&snap, &manifest, &sources);
    let d = digest_of(&dir, "svc.go");
    let ev = |seq: u64, lo: u64, hi: u64| AgentEvent {
        schema: "reposuite.agent-event.v1".into(),
        event_id: format!("s:{seq}"),
        session_id: "s".into(),
        investigation_id: Some("inv".into()),
        project_id: None,
        repository_id: Some("r".into()),
        repo_head: Some("h".into()),
        sequence: seq,
        timestamp: "t".into(),
        source: repodex::agent_event::model::SourceProvenance {
            runtime: "test".into(),
            adapter: "test".into(),
            adapter_version: "1".into(),
            native_session_id: None,
        },
        event_type: "source_observed".into(),
        data: serde_json::json!({"path":"svc.go","observation_kind":"explicit_read",
            "line_start":lo,"line_end":hi,"file_content_digest":d}),
        content_digest: None,
    };
    let mut store = SymbolExposureStore::open(dir.path());
    derive_symbol_exposures(&[ev(1, 8, 10), ev(2, 7, 7)], &provider, &mut store);
    assert_eq!(store.diag.source_observed_events, 2);
    assert_eq!(store.diag.resolved_indexed_hit, 2);
    let doc = store.docs.get("inv").unwrap();
    assert!(doc.exposures.iter().any(|e| e.symbol_name == "Outer"));
}
