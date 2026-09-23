//! SymbolExposure performance measurement (§56-§63). Reports observed
//! durations for the four required cases — no assertions, writes JSON.

mod support;

use std::time::Instant;

use repodex::agent_event::model::ObservationKind;
use repodex::parser::{Analyzer, AnalyzerConfig};
use repodex::repository::{artifact, build_snapshot, digest, BuildOptions};
use repodex::symbol_exposure::{resolve_observation, Observation, SourceStore, SymbolMapProvider};
use support::TempDir;

fn obs(path: &str, lo: u64, hi: u64, digest: Option<String>, seq: u64) -> Observation {
    Observation {
        investigation_id: "inv".into(),
        session_id: "s".into(),
        repository_id: "r".into(),
        repo_head: Some("h".into()),
        source_event_sequence: seq,
        path: path.into(),
        line_start: Some(lo),
        line_end: Some(hi),
        file_content_digest: digest,
        kind: ObservationKind::ExplicitRead,
    }
}

fn pct(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let i = ((sorted.len() - 1) as f64 * p).round() as usize;
    sorted[i.min(sorted.len() - 1)]
}

fn summary(times_ms: &[f64]) -> serde_json::Value {
    let mut v = times_ms.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mean = v.iter().sum::<f64>() / v.len().max(1) as f64;
    serde_json::json!({
        "n": v.len(), "mean_ms": mean, "p50_ms": pct(&v, 0.50),
        "p95_ms": pct(&v, 0.95), "p99_ms": pct(&v, 0.99),
        "max_ms": v.last().copied().unwrap_or(0.0),
    })
}

/// A ~realistic Go file with many decls/calls (matches GTW scale).
fn go_source(nfuncs: usize) -> String {
    let mut s = String::from("package svc\n\ntype Config struct{ Name string }\n\n");
    for i in 0..nfuncs {
        s.push_str(&format!(
            "func Worker{i}(c *Config) int {{\n\tx := helper{i}(c)\n\tif x > 0 {{\n\t\treturn use(x, c.Name)\n\t}}\n\treturn 0\n}}\n\n",
        ));
        s.push_str(&format!(
            "func helper{i}(c *Config) int {{ return {i} }}\n\n"
        ));
    }
    s
}

#[test]
fn symbol_resolution_perf() {
    let dir = TempDir::new("sexp-perf");
    let src = go_source(120); // ~240 decls + ~360 calls
    dir.write("svc.go", src.as_bytes());
    let snap = dir.path().join("snap");
    let analyzer = Analyzer::new(AnalyzerConfig::default()).unwrap();
    build_snapshot(&analyzer, dir.path(), &snap, BuildOptions::default()).unwrap();
    let manifest = artifact::read_manifest(&snap).unwrap();
    let indexed_digest = digest::content_digest(src.as_bytes());

    let mut out = serde_json::json!({});

    // ---- Case A + C: indexed digest hit (cold then repeated) ---------------
    {
        let sources = SourceStore::default();
        let provider = SymbolMapProvider::new(&snap, &manifest, &sources);
        let mut hit = Vec::new();
        for i in 0..500u64 {
            let o = obs(
                "svc.go",
                (i % 400) + 1,
                (i % 400) + 30,
                Some(indexed_digest.clone()),
                i,
            );
            let t = Instant::now();
            let _ = resolve_observation(&provider, &o);
            hit.push(t.elapsed().as_secs_f64() * 1000.0);
        }
        out["A_indexed_digest_hit"] = summary(&hit);
        assert_eq!(
            provider.diag.borrow().new_parse,
            0,
            "no reparse on index hit"
        );
    }

    // ---- Case B: new/unseen version parse + C: repeated (cached) -----------
    {
        let mut sources = SourceStore::default();
        let src_b = go_source(120) + "// variant b\n";
        let db = sources.put(src_b.as_bytes().to_vec());
        let provider = SymbolMapProvider::new(&snap, &manifest, &sources);
        // First observation -> parses the unseen version once.
        let t = Instant::now();
        let _ = resolve_observation(&provider, &obs("svc.go", 5, 35, Some(db.clone()), 0));
        out["B_new_version_parse"] = serde_json::json!({
            "n": 1, "parse_ms": t.elapsed().as_secs_f64() * 1000.0,
            "new_parse_count": provider.diag.borrow().new_parse,
        });
        // Repeated observations of the SAME digest -> cached map (no reparse).
        let mut cached = Vec::new();
        for i in 1..500u64 {
            let o = obs("svc.go", (i % 400) + 1, (i % 400) + 30, Some(db.clone()), i);
            let t = Instant::now();
            let _ = resolve_observation(&provider, &o);
            cached.push(t.elapsed().as_secs_f64() * 1000.0);
        }
        out["C_repeated_same_version_cached"] = summary(&cached);
        assert_eq!(
            provider.diag.borrow().new_parse,
            1,
            "unseen version parsed once, then cached"
        );
    }

    // ---- Case D: rapid version churn (K distinct versions) -----------------
    {
        let mut sources = SourceStore::default();
        let mut digests = Vec::new();
        for v in 0..50u32 {
            let s = go_source(120) + &format!("// churn {v}\n");
            digests.push(sources.put(s.as_bytes().to_vec()));
        }
        let provider = SymbolMapProvider::new(&snap, &manifest, &sources);
        let mut parse_t = Vec::new();
        for (i, d) in digests.iter().enumerate() {
            let o = obs("svc.go", 5, 35, Some(d.clone()), i as u64);
            let t = Instant::now();
            let _ = resolve_observation(&provider, &o);
            parse_t.push(t.elapsed().as_secs_f64() * 1000.0);
        }
        out["D_rapid_edit_parse"] = summary(&parse_t);
        out["D_distinct_versions"] = serde_json::json!(digests.len());
        out["D_new_parse_total"] = serde_json::json!(provider.diag.borrow().new_parse);
    }

    // ---- Throughput: background worker rate on cached/indexed mix ---------
    {
        let sources = SourceStore::default();
        let provider = SymbolMapProvider::new(&snap, &manifest, &sources);
        let n = 10_000u64;
        let t = Instant::now();
        for i in 0..n {
            let o = obs(
                "svc.go",
                (i % 400) + 1,
                (i % 400) + 30,
                Some(indexed_digest.clone()),
                i,
            );
            let _ = resolve_observation(&provider, &o);
        }
        let el = t.elapsed().as_secs_f64();
        out["throughput"] = serde_json::json!({
            "events": n, "seconds": el, "events_per_sec": n as f64 / el,
        });
    }

    // ---- storage cost ------------------------------------------------------
    let src_bytes = src.len() as u64;
    out["storage"] = serde_json::json!({
        "source_file_bytes": src_bytes,
        "naive_copy_per_read_500": src_bytes * 500,
        "dedup_unique_versions_held": "1 indexed + N unseen (dedup by digest)",
        "analysis_json_bytes": std::fs::metadata(
            snap.join("files").join(format!("{}.json",
                manifest.files.iter().find(|f| f.relative_path == "svc.go")
                    .map(|f| f.object_key.clone()).unwrap_or_default()))).map(|m| m.len()).unwrap_or(0),
    });

    let dst = std::env::var("SEXP_PERF_OUT")
        .unwrap_or_else(|_| "/tmp/repodex-symbol-exposure-v1/PERF_LOCAL.json".into());
    std::fs::create_dir_all(std::path::Path::new(&dst).parent().unwrap()).ok();
    std::fs::write(&dst, serde_json::to_string_pretty(&out).unwrap()).unwrap();
    println!("{}", serde_json::to_string_pretty(&out).unwrap());
}
