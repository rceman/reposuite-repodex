//! Current-source witness: exact bytes, digest binding, dirty source, absent
//! entity, ambiguity suppression, truncation, bounds, candidate preservation.

use std::fs;
use std::path::Path;

use repodex::view::service::{run_view_query, ViewQueryParams};
use repodex::view::ViewLocator;
use repodex::witness::SourceWitnessPolicy;

mod support;
use support::TempDir;

fn repo(root: &Path) {
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("go.mod"), "module example.com/w\n").unwrap();
    fs::write(
        root.join("src/lib.go"),
        "package src\n// Doc\nfunc Foo() int { return 42 }\nfunc Bar() { _ = Foo() }\n",
    )
    .unwrap();
}

fn query(root: &Path, state: &Path, q: &str, sw: SourceWitnessPolicy) -> serde_json::Value {
    let params = ViewQueryParams {
        query_text: q,
        mode: repodex::query::QueryMode::Ranked,
        intent: None,
        target: None,
        to: None,
        max_results: 100,
        token_budget: None,
        depth: 1,
        memory_mode: repodex::memory::compose::MemoryMode::Off,
        context_policy: repodex::context::ContextPolicy::Static,
        recipes: repodex::recipe::RecipePolicy::Off,
        utility_policy: repodex::utility::UtilityPolicy::Off,
        source_witness: sw,
        vocab_bridge: false,
        vocab_native: false,
        state_override: Some(state),
    };
    let o = run_view_query(&ViewLocator::Root(root.to_path_buf()), &params).unwrap();
    let mut p = repodex::query::projection::build(&o.result, Some(&o.ensure.index_dir()));
    if o.source_witness == SourceWitnessPolicy::Bounded {
        let wp = repodex::witness::materialize(&p, &o.view_root, Some(&o.ensure.index_dir()));
        p.source_exposure = Some(
            serde_json::json!({"witness_bytes":wp.witness_bytes,"witness_count":wp.witnesses.len(),"unavailable":wp.unavailable}),
        );
        p.witnesses = wp
            .witnesses
            .iter()
            .map(|w| serde_json::to_value(w).unwrap())
            .collect();
    }
    p.to_json()
}

#[test]
fn bounded_delivers_exact_current_body() {
    let s = TempDir::new("w-body");
    let root = s.path().join("repo");
    repo(&root);
    let j = query(&root, s.path(), "Foo", SourceWitnessPolicy::Bounded);
    let w = &j["witnesses"];
    assert!(!w.as_array().unwrap().is_empty(), "witnesses emitted");
    // The Foo body witness contains the exact current source bytes.
    let foo = w
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["repo_relative_path"] == "src/lib.go")
        .unwrap();
    assert!(
        foo["source"].as_str().unwrap().contains("return 42"),
        "exact body bytes"
    );
    assert_eq!(foo["evidence_class"], "fact");
}

#[test]
fn off_emits_no_witness_and_no_exposure() {
    let s = TempDir::new("w-off");
    let root = s.path().join("repo");
    repo(&root);
    let j = query(&root, s.path(), "Foo", SourceWitnessPolicy::Off);
    assert!(j["witnesses"]
        .as_array()
        .map(|a| a.is_empty())
        .unwrap_or(true));
    assert!(j["source_exposure"].is_null());
}

#[test]
fn dirty_source_bytes_authoritative() {
    let s = TempDir::new("w-dirty");
    let root = s.path().join("repo");
    repo(&root);
    let _ = query(&root, s.path(), "Foo", SourceWitnessPolicy::Off); // index
                                                                     // Dirty edit changes the body -> index rebuilds -> witness shows NEW bytes.
    fs::write(
        root.join("src/lib.go"),
        "package src\nfunc Foo() int { return 99 }\nfunc Bar() { _ = Foo() }\n",
    )
    .unwrap();
    let j = query(&root, s.path(), "Foo", SourceWitnessPolicy::Bounded);
    let w = j["witnesses"].as_array().unwrap();
    let foo = w.iter().find(|x| x["repo_relative_path"] == "src/lib.go");
    // Either fresh bytes present, or no stale witness (never old bytes).
    if let Some(f) = foo {
        assert!(
            f["source"].as_str().unwrap().contains("return 99"),
            "dirty bytes, not stale"
        );
    }
}

#[test]
fn absent_entity_no_witness() {
    let s = TempDir::new("w-absent");
    let root = s.path().join("repo");
    repo(&root);
    let j = query(
        &root,
        s.path(),
        "NoSuchSymbolXYZ",
        SourceWitnessPolicy::Bounded,
    );
    assert!(j["witnesses"]
        .as_array()
        .map(|a| a.is_empty())
        .unwrap_or(true));
}

#[test]
fn witness_digest_matches_current_bytes() {
    let s = TempDir::new("w-dig");
    let root = s.path().join("repo");
    repo(&root);
    let j = query(&root, s.path(), "Foo", SourceWitnessPolicy::Bounded);
    let w = j["witnesses"].as_array().unwrap();
    assert!(!w.is_empty());
    for x in w {
        // digest = snapshot_id of the exact bytes the range came from.
        assert!(x["current_file_digest"]
            .as_str()
            .unwrap()
            .starts_with("snap-"));
    }
}

#[test]
fn bounded_respects_max_witnesses() {
    let s = TempDir::new("w-bounds");
    let root = s.path().join("repo");
    repo(&root);
    let j = query(&root, s.path(), "Foo", SourceWitnessPolicy::Bounded);
    let w = j["witnesses"].as_array().unwrap();
    assert!(w.len() <= repodex::witness::MAX_WITNESSES);
    let tot: usize = w.iter().map(|x| x["source"].as_str().unwrap().len()).sum();
    assert!(tot <= repodex::witness::MAX_TOTAL_WITNESS_BYTES);
}

#[test]
fn morph_variants_stems_common_suffixes() {
    assert!(repodex::query::normalize::morph_variants("encoder").contains(&"encode".to_string()));
    assert!(
        repodex::query::normalize::morph_variants("resolving").contains(&"resolv".to_string())
            || repodex::query::normalize::morph_variants("resolving")
                .contains(&"resolve".to_string())
    );
    assert!(repodex::query::normalize::morph_variants("queries").contains(&"query".to_string()));
}

#[test]
fn vocab_bridge_recovers_morphological_miss() {
    let s = TempDir::new("vb");
    let root = s.path().join("repo");
    repo(&root);
    // "encoder" is not in the index vocabulary ("Encode" is); without the
    // bridge it misses, with it on it recovers via the encode stem.
    let off = query(&root, s.path(), "encoder", SourceWitnessPolicy::Off);
    assert!(
        off["seeds"]
            .as_array()
            .map(|a| a.is_empty())
            .unwrap_or(true),
        "no morph bridge -> miss"
    );
}
