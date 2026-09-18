//! TASK 3C: derived Rust local plain-name call candidates.
//!
//! The gates that matter most here are:
//!
//! * the curated fixture asserts **complete** candidate sets, not just the
//!   presence of the candidate we hoped for;
//! * the negative suite proves RepoDex does *not* manufacture a candidate when
//!   the bounded rule does not support one — a `FALSE_CANDIDATE` is the most
//!   dangerous defect this layer can have;
//! * `SingleCandidate` is a dedicated cardinality: it is never `Exact` and
//!   never a one-element `Ambiguous`, because a candidate is not a resolved
//!   call target;
//! * the determinism and equivalence gates prove a candidate artifact is a
//!   pure function of the snapshot bytes plus the link-artifact bytes;
//! * the integrity gates prove a corrupted, reordered, or stale-dependency
//!   artifact fails verification.

mod support;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use repodex::candidates::{
    build_candidates, candidate_rule, read_manifest, read_records, verify, CallCandidateRecord,
    CandidateError, CandidateIndex, CandidateOutcome,
};
use repodex::links::FactKind;
use repodex::repository::{build_snapshot, update_snapshot, BuildOptions, RepositoryManifest};
use support::TempDir;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn fixture_root() -> PathBuf {
    support::fixture("callcandidates").join("rust")
}

fn store(temp: &TempDir, name: &str) -> PathBuf {
    temp.path().join("store").join(name)
}

fn snapshot(root: &Path, output: &Path) -> RepositoryManifest {
    build_snapshot(&support::analyzer(), root, output, BuildOptions::default())
        .expect("snapshot build")
        .manifest
}

fn link(snapshot_dir: &Path, repository: &Path, output: &Path) {
    repodex::links::build_links(snapshot_dir, Some(repository), output).expect("link build");
}

fn candidate(
    snapshot_dir: &Path,
    links_dir: &Path,
    output: &Path,
) -> repodex::candidates::CandidateBuildOutcome {
    build_candidates(snapshot_dir, links_dir, output).expect("candidate build")
}

/// Build and load the snapshot, links and candidates for the committed fixture.
fn fixture_candidates(temp: &TempDir) -> (PathBuf, PathBuf, CandidateIndex) {
    let root = fixture_root();
    let snapshot_dir = store(temp, "snap");
    snapshot(&root, &snapshot_dir);
    let links_dir = store(temp, "links");
    link(&snapshot_dir, &root, &links_dir);
    let candidates_dir = store(temp, "candidates");
    candidate(&snapshot_dir, &links_dir, &candidates_dir);
    let index = CandidateIndex::load(&candidates_dir).expect("load candidates");
    (snapshot_dir, links_dir, index)
}

/// The record for one call, located by file, written callee and scope suffix.
fn record_by<'a>(
    index: &'a CandidateIndex,
    path: &str,
    written: &str,
    scope_suffix: &str,
) -> &'a CallCandidateRecord {
    index
        .records()
        .iter()
        .find(|record| {
            record.source.relative_path == path
                && record.written == written
                && record.provenance.scope_path.ends_with(scope_suffix)
        })
        .unwrap_or_else(|| panic!("no candidate record for {path} `{written}` in `{scope_suffix}`"))
}

/// `path#id:name` for one candidate target.
fn target_summary(candidate: &repodex::candidates::CandidateTarget) -> String {
    format!(
        "{}#{}:{}",
        candidate.relative_path, candidate.declaration_id, candidate.name
    )
}

/// `(outcome, [candidates])` for one call.
fn outcome_of(record: &CallCandidateRecord) -> (String, BTreeSet<String>) {
    (
        record.outcome.as_str().to_string(),
        record
            .outcome
            .candidates()
            .iter()
            .map(target_summary)
            .collect(),
    )
}

fn assert_outcome(
    index: &CandidateIndex,
    path: &str,
    written: &str,
    scope_suffix: &str,
    expected_outcome: &str,
    expected_candidates: &[&str],
) {
    let record = record_by(index, path, written, scope_suffix);
    let (outcome, candidates) = outcome_of(record);
    let expected: BTreeSet<String> = expected_candidates.iter().map(|s| s.to_string()).collect();
    assert_eq!(
        outcome, expected_outcome,
        "{path} `{written}` in `{scope_suffix}`"
    );
    assert_eq!(
        candidates, expected,
        "{path} `{written}` in `{scope_suffix}` candidate set"
    );
}

fn read_bytes(path: &Path) -> Vec<u8> {
    std::fs::read(path).expect("read file")
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("create destination");
    for entry in std::fs::read_dir(from).expect("read source") {
        let entry = entry.expect("dir entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("file type").is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).expect("copy file");
        }
    }
}

/// Rewrite the candidate records file in place.
fn rewrite_records(dir: &Path, records: &[CallCandidateRecord]) {
    let mut buffer = Vec::new();
    for record in records {
        serde_json::to_writer(&mut buffer, record).expect("serialize");
        buffer.push(b'\n');
    }
    std::fs::write(dir.join("call_candidates.jsonl"), buffer).expect("write records");
}

/// Rewrite the candidate manifest in place, refreshing the digest so it stays
/// internally consistent.
fn mutate_candidate_manifest(
    dir: &Path,
    mutate: impl FnOnce(&mut repodex::candidates::CandidateManifest),
) {
    let mut manifest = read_manifest(dir).expect("load manifest");
    let records = read_records(dir).expect("load records");
    mutate(&mut manifest);
    manifest.refresh_candidate_digest(&records);
    std::fs::write(
        dir.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest).expect("serialize"),
    )
    .expect("write manifest");
}

/// A valid-format but wrong sha256 digest string.
const FOREIGN_DIGEST: &str =
    "sha256:0000000000000000000000000000000000000000000000000000000000000001";

// ---------------------------------------------------------------------------
// Curated fixture: complete candidate sets
// ---------------------------------------------------------------------------

#[test]
fn module_level_function_is_a_single_candidate() {
    let temp = TempDir::new("t3c-mod");
    let (_snap, _links, index) = fixture_candidates(&temp);
    assert_outcome(
        &index,
        "src/lib.rs",
        "helper",
        "(file)::run",
        "single_candidate",
        &["src/lib.rs#4:helper"],
    );
}

#[test]
fn nested_function_is_a_single_candidate_and_shadows_the_module_one() {
    let temp = TempDir::new("t3c-shadow");
    let (_snap, _links, index) = fixture_candidates(&temp);
    // `outer`'s nested `fn helper` (declaration 10) wins at level 0; the
    // module-level `helper` (declaration 4) must NOT also be returned.
    assert_outcome(
        &index,
        "src/lib.rs",
        "helper",
        "(file)::outer",
        "single_candidate",
        &["src/lib.rs#10:helper"],
    );
}

#[test]
fn same_name_in_an_unrelated_module_is_not_a_candidate() {
    let temp = TempDir::new("t3c-module");
    let (_snap, _links, index) = fixture_candidates(&temp);
    // `a::run`'s helper() -> `a::helper` only; `b::helper` is in a different
    // module and must not appear.
    assert_outcome(
        &index,
        "src/a.rs",
        "helper",
        "(file)::run",
        "single_candidate",
        &["src/a.rs#0:helper"],
    );
    // And no record anywhere targets `src/b.rs`'s helper (declaration 0).
    assert!(
        index.targeting_declaration("src/b.rs", 0).is_empty(),
        "no call may target a same-named function in an unrelated module"
    );
}

#[test]
fn inline_module_isolation_is_preserved() {
    let temp = TempDir::new("t3c-inline");
    let (_snap, _links, index) = fixture_candidates(&temp);
    // `left::run` -> `left::helper`, never `right::helper`.
    assert_outcome(
        &index,
        "src/inline.rs",
        "helper",
        "(file)::left::run",
        "single_candidate",
        &["src/inline.rs#1:helper"],
    );
    // `lonely_mod::run` -> NoCandidate: `helper` is not in `lonely_mod`, and the
    // module boundary prevents borrowing `left::helper`/`right::helper`.
    assert_outcome(
        &index,
        "src/inline.rs",
        "helper",
        "(file)::lonely_mod::run",
        "no_candidate",
        &[],
    );
    // A file-level call still reaches a file-level function.
    assert_outcome(
        &index,
        "src/inline.rs",
        "file_helper",
        "(file)::file_run",
        "single_candidate",
        &["src/inline.rs#7:file_helper"],
    );
}

#[test]
fn a_function_in_the_crate_root_is_not_a_candidate_inside_a_module() {
    let temp = TempDir::new("t3c-lonely");
    let (_snap, _links, index) = fixture_candidates(&temp);
    // `mod lonely` calls `helper()`, but `helper` lives at the crate root.
    // The `mod lonely` boundary confines the search, so this is NoCandidate
    // even though a same-named function exists in another module.
    assert_outcome(
        &index,
        "src/lonely.rs",
        "helper",
        "(file)::run",
        "no_candidate",
        &[],
    );
}

#[test]
fn missing_name_is_no_candidate() {
    let temp = TempDir::new("t3c-missing");
    let (_snap, _links, index) = fixture_candidates(&temp);
    assert_outcome(
        &index,
        "src/lib.rs",
        "absent",
        "(file)::missing",
        "no_candidate",
        &[],
    );
}

#[test]
fn a_local_closure_variable_is_not_a_function_candidate() {
    let temp = TempDir::new("t3c-closure");
    let (_snap, _links, index) = fixture_candidates(&temp);
    assert_outcome(
        &index,
        "src/scope.rs",
        "closure",
        "(file)::run",
        "no_candidate",
        &[],
    );
    // A local fn-pointer variable is also not a candidate.
    assert_outcome(
        &index,
        "src/scope.rs",
        "f",
        "(file)::run",
        "no_candidate",
        &[],
    );
    // A tuple-struct construction is not a free-function candidate.
    assert_outcome(
        &index,
        "src/scope.rs",
        "User",
        "(file)::run",
        "no_candidate",
        &[],
    );
}

#[test]
fn a_call_inside_an_impl_method_reaches_the_module_function() {
    let temp = TempDir::new("t3c-impl");
    let (_snap, _links, index) = fixture_candidates(&temp);
    // `impl` is transparent for module-item visibility.
    assert_outcome(
        &index,
        "src/scope.rs",
        "helper",
        "(file)::(impl S)::caller",
        "single_candidate",
        &["src/scope.rs#0:helper"],
    );
}

#[test]
fn imports_and_reexports_produce_no_candidate() {
    let temp = TempDir::new("t3c-imports");
    let (_snap, _links, index) = fixture_candidates(&temp);
    assert_outcome(
        &index,
        "src/lib.rs",
        "imported_fn",
        "(file)::run_imported",
        "no_candidate",
        &[],
    );
    assert_outcome(
        &index,
        "src/lib.rs",
        "reexported_fn",
        "(file)::run_reexport",
        "no_candidate",
        &[],
    );
    // No record anywhere may target the import/re-export declarations.
    for decl_id in 0..2 {
        assert!(
            index
                .targeting_declaration("src/other.rs", decl_id)
                .is_empty(),
            "no call may resolve an imported or re-exported function under this rule"
        );
    }
}

#[test]
fn non_plain_name_calls_are_out_of_scope() {
    let temp = TempDir::new("t3c-oos");
    let (_snap, _links, index) = fixture_candidates(&temp);
    let out_of_scope: Vec<(&str, &str)> = vec![
        ("crate::helper", "(file)::run"),
        ("self::helper", "(file)::run"),
        ("obj.method", "(file)::run"),
        ("S::assoc", "(file)::run"),
        ("(helper)", "(file)::run"),
        ("produce()", "(file)::run"),
        ("my_macro", "(file)::run"),
    ];
    for (written, scope) in out_of_scope {
        assert_outcome(&index, "src/edge.rs", written, scope, "out_of_scope", &[]);
    }
    // The inner `produce()` of `produce()()` is a real plain-name call with no
    // local function -> NoCandidate.
    assert_outcome(
        &index,
        "src/edge.rs",
        "produce",
        "(file)::run",
        "no_candidate",
        &[],
    );
}

#[test]
fn duplicate_functions_in_one_scope_are_multiple_candidates() {
    let temp = TempDir::new("t3c-dup");
    let (_snap, _links, index) = fixture_candidates(&temp);
    assert_outcome(
        &index,
        "src/dup.rs",
        "dup",
        "(file)::run",
        "multiple_candidates",
        &["src/dup.rs#0:dup", "src/dup.rs#1:dup"],
    );
}

#[test]
fn recovered_source_still_yields_candidates() {
    let temp = TempDir::new("t3c-recovered");
    let (_snap, _links, index) = fixture_candidates(&temp);
    assert_outcome(
        &index,
        "src/broken.rs",
        "helper",
        "(file)::run",
        "single_candidate",
        &["src/broken.rs#0:helper"],
    );
}

#[test]
fn an_extern_block_function_is_not_a_candidate() {
    let temp = TempDir::new("t3c-extern");
    let (_snap, _links, index) = fixture_candidates(&temp);
    // `ext_helper` is declared inside an `extern "C"` block. The bounded rule
    // searches module-level `function` declarations only, and the extern block
    // is a separate scope, so this is a documented bounded miss.
    assert_outcome(
        &index,
        "src/ext.rs",
        "ext_helper",
        "(file)::run",
        "no_candidate",
        &[],
    );
}

// ---------------------------------------------------------------------------
// Candidate-model semantics
// ---------------------------------------------------------------------------

#[test]
fn a_single_candidate_is_never_exact_and_never_ambiguous() {
    let temp = TempDir::new("t3c-semantics");
    let (_snap, _links, index) = fixture_candidates(&temp);
    let record = record_by(&index, "src/lib.rs", "helper", "(file)::run");
    // The cardinality is its own thing, serialized as `single_candidate`.
    assert!(matches!(
        record.outcome,
        CandidateOutcome::SingleCandidate { .. }
    ));
    assert_eq!(record.outcome.as_str(), "single_candidate");
    let serialized = serde_json::to_string(&record.outcome).expect("serialize");
    assert!(serialized.contains("\"outcome\":\"single_candidate\""));
    assert!(!serialized.contains("\"exact\""));
    assert!(!serialized.contains("\"ambiguous\""));
    // And a candidate record is not a call edge: it carries no resolved target.
    assert!(record.record_id.starts_with("cand-"));
}

#[test]
fn every_record_is_a_candidate_record_not_a_call_edge() {
    let temp = TempDir::new("t3c-no-edge");
    let (_snap, _links, index) = fixture_candidates(&temp);
    for record in index.records() {
        assert_eq!(
            record.rule_id,
            candidate_rule::RUST_CALL_LOCAL_FUNCTION_CANDIDATE
        );
        assert!(record.record_id.starts_with("cand-"));
        // Candidates are the only targets; the outcome vocabulary is candidate
        // vocabulary, never resolution vocabulary.
        assert!(matches!(
            record.outcome,
            CandidateOutcome::NoCandidate { .. }
                | CandidateOutcome::SingleCandidate { .. }
                | CandidateOutcome::MultipleCandidates { .. }
                | CandidateOutcome::OutOfScope { .. }
        ));
    }
}

#[test]
fn the_rule_registry_documents_that_one_candidate_is_not_resolution() {
    let temp = TempDir::new("t3c-registry");
    let (_snap, _links, index) = fixture_candidates(&temp);
    let manifest = &index.manifest;
    // Every emitted rule id is documented.
    let documented: BTreeSet<&str> = manifest
        .rules
        .iter()
        .map(|rule| rule.rule_id.as_str())
        .collect();
    for record in index.records() {
        assert!(
            documented.contains(record.rule_id.as_str()),
            "rule {} is undocumented",
            record.rule_id
        );
    }
    let rule = manifest
        .rule(candidate_rule::RUST_CALL_LOCAL_FUNCTION_CANDIDATE)
        .expect("rule documented");
    assert!(rule.single_candidate_meaning.contains("NOT proof"));
    assert!(rule.no_candidate_meaning.contains("does not mean"));
    // The registry names the known exclusions explicitly.
    let exclusions = rule.known_exclusions.join(";");
    assert!(exclusions.contains("imports and re-exports"));
    assert!(exclusions.contains("closures and local callable variables"));
    assert!(exclusions.contains("extern block"));
}

// ---------------------------------------------------------------------------
// Query API
// ---------------------------------------------------------------------------

#[test]
fn the_query_api_filters_by_cardinality_source_and_target() {
    let temp = TempDir::new("t3c-query");
    let (_snap, _links, index) = fixture_candidates(&temp);
    assert!(!index.single().is_empty());
    assert!(!index.multiple().is_empty());
    assert!(!index.none().is_empty());
    assert!(!index.out_of_scope().is_empty());
    assert!(!index.from_source("src/lib.rs").is_empty());
    assert!(index.from_source("src/nope.rs").is_empty());
    assert!(!index
        .by_rule(candidate_rule::RUST_CALL_LOCAL_FUNCTION_CANDIDATE)
        .is_empty());
    // `a.rs`'s helper is a candidate target; `b.rs`'s is not.
    assert_eq!(index.targeting_file("src/a.rs").len(), 1);
    assert!(index.targeting_file("src/b.rs").is_empty());
    assert_eq!(index.targeting_declaration("src/a.rs", 0).len(), 1);
    let counts = index.cardinality_counts();
    assert_eq!(
        counts.single_candidate
            + counts.multiple_candidates
            + counts.no_candidate
            + counts.out_of_scope,
        index.records().len() as u64
    );
}

// ---------------------------------------------------------------------------
// Determinism
// ---------------------------------------------------------------------------

#[test]
fn repeated_builds_are_byte_identical() {
    let temp = TempDir::new("t3c-det");
    let root = fixture_root();
    let snapshot_dir = store(&temp, "snap");
    snapshot(&root, &snapshot_dir);
    let links_dir = store(&temp, "links");
    link(&snapshot_dir, &root, &links_dir);
    let first = store(&temp, "c1");
    let a = candidate(&snapshot_dir, &links_dir, &first);
    let second = store(&temp, "c2");
    let b = candidate(&snapshot_dir, &links_dir, &second);
    assert_eq!(a.manifest.candidate_digest, b.manifest.candidate_digest);
    assert_eq!(
        read_bytes(&first.join("manifest.json")),
        read_bytes(&second.join("manifest.json"))
    );
    assert_eq!(
        read_bytes(&first.join("call_candidates.jsonl")),
        read_bytes(&second.join("call_candidates.jsonl"))
    );
}

#[test]
fn builds_are_identical_across_output_directories() {
    let temp = TempDir::new("t3c-outdir");
    let root = fixture_root();
    let snapshot_dir = store(&temp, "snap");
    snapshot(&root, &snapshot_dir);
    let links_dir = store(&temp, "links");
    link(&snapshot_dir, &root, &links_dir);
    let a = candidate(
        &snapshot_dir,
        &links_dir,
        &store(&temp, "out/here/candidates"),
    );
    let b = candidate(
        &snapshot_dir,
        &links_dir,
        &store(&temp, "out/elsewhere/candidates"),
    );
    assert_eq!(a.manifest.candidate_digest, b.manifest.candidate_digest);
}

#[test]
fn builds_are_identical_across_checkout_roots() {
    let temp = TempDir::new("t3c-root");
    let root_a = temp.path().join("root-a");
    let root_b = temp.path().join("root-b");
    copy_tree(&fixture_root(), &root_a);
    copy_tree(&fixture_root(), &root_b);
    let digest = |root: &Path, label: &str| {
        let snapshot_dir = store(&temp, &format!("{label}-snap"));
        snapshot(root, &snapshot_dir);
        let links_dir = store(&temp, &format!("{label}-links"));
        link(&snapshot_dir, root, &links_dir);
        let candidates_dir = store(&temp, &format!("{label}-candidates"));
        let outcome = candidate(&snapshot_dir, &links_dir, &candidates_dir);
        (
            outcome.manifest.candidate_digest,
            read_bytes(&candidates_dir.join("call_candidates.jsonl")),
        )
    };
    let (digest_a, bytes_a) = digest(&root_a, "a");
    let (digest_b, bytes_b) = digest(&root_b, "b");
    assert_eq!(
        digest_a, digest_b,
        "candidate digest must not depend on the checkout root"
    );
    assert_eq!(
        bytes_a, bytes_b,
        "candidate bytes must not depend on the checkout root"
    );
}

// ---------------------------------------------------------------------------
// Fresh-vs-updated pipeline equivalence
// ---------------------------------------------------------------------------

fn candidate_repo(temp: &TempDir, name: &str) -> PathBuf {
    let root = temp.path().join(name);
    std::fs::create_dir_all(root.join("src")).expect("mkdir");
    std::fs::write(
        root.join("src/lib.rs"),
        b"mod util;\n\nuse crate::util::helper;\n\nfn helper() {}\nfn run() { helper(); }\n",
    )
    .expect("write");
    std::fs::write(root.join("src/util.rs"), b"pub fn helper() {}\n").expect("write");
    root
}

fn assert_update_equals_fresh(
    label: &str,
    root: &Path,
    mutate: impl FnOnce(&Path),
    temp: &TempDir,
) {
    let base_snapshot = store(temp, &format!("{label}-base-snap"));
    snapshot(root, &base_snapshot);
    let base_links = store(temp, &format!("{label}-base-links"));
    link(&base_snapshot, root, &base_links);

    mutate(root);

    // Path A: incremental update, then a link rebuild and a candidate rebuild.
    let updated_snapshot = store(temp, &format!("{label}-updated-snap"));
    update_snapshot(
        &support::analyzer(),
        root,
        &base_snapshot,
        &updated_snapshot,
        BuildOptions::default(),
    )
    .expect("snapshot update");
    let updated_links = store(temp, &format!("{label}-updated-links"));
    link(&updated_snapshot, root, &updated_links);
    let updated_candidates = store(temp, &format!("{label}-updated-candidates"));
    let updated = candidate(&updated_snapshot, &updated_links, &updated_candidates);

    // Path B: a fresh snapshot of the same final bytes, then links + candidates.
    let fresh_snapshot = store(temp, &format!("{label}-fresh-snap"));
    snapshot(root, &fresh_snapshot);
    let fresh_links = store(temp, &format!("{label}-fresh-links"));
    link(&fresh_snapshot, root, &fresh_links);
    let fresh_candidates = store(temp, &format!("{label}-fresh-candidates"));
    let fresh = candidate(&fresh_snapshot, &fresh_links, &fresh_candidates);

    assert_eq!(
        updated.manifest.snapshot_digest, fresh.manifest.snapshot_digest,
        "{label}: updated and fresh snapshots must agree"
    );
    assert_eq!(
        updated.manifest.link_digest, fresh.manifest.link_digest,
        "{label}: updated and fresh link artifacts must agree"
    );
    assert_eq!(
        updated.manifest.candidate_digest, fresh.manifest.candidate_digest,
        "{label}: updated and fresh candidate artifacts must agree"
    );
    assert_eq!(
        read_bytes(&updated_candidates.join("call_candidates.jsonl")),
        read_bytes(&fresh_candidates.join("call_candidates.jsonl")),
        "{label}: candidate bytes must agree"
    );
}

#[test]
fn update_equals_fresh_for_no_changes() {
    let temp = TempDir::new("t3c-eq-none");
    let root = candidate_repo(&temp, "repo");
    assert_update_equals_fresh("none", &root, |_| {}, &temp);
}

#[test]
fn update_equals_fresh_for_a_local_function_added() {
    let temp = TempDir::new("t3c-eq-addfn");
    let root = candidate_repo(&temp, "repo");
    assert_update_equals_fresh(
        "addfn",
        &root,
        |root| {
            let path = root.join("src/lib.rs");
            let mut source = std::fs::read_to_string(&path).expect("read");
            source.push_str("fn added() {}\n");
            std::fs::write(path, source).expect("write");
        },
        &temp,
    );
}

#[test]
fn update_equals_fresh_for_a_local_function_removed() {
    let temp = TempDir::new("t3c-eq-delfn");
    let root = candidate_repo(&temp, "repo");
    assert_update_equals_fresh(
        "delfn",
        &root,
        |root| {
            std::fs::write(
                root.join("src/lib.rs"),
                b"mod util;\n\nuse crate::util::helper;\n\nfn run() { helper(); }\n",
            )
            .expect("write");
        },
        &temp,
    );
}

#[test]
fn update_equals_fresh_for_a_local_function_renamed() {
    let temp = TempDir::new("t3c-eq-renfn");
    let root = candidate_repo(&temp, "repo");
    assert_update_equals_fresh(
        "renfn",
        &root,
        |root| {
            std::fs::write(
                root.join("src/lib.rs"),
                b"mod util;\n\nuse crate::util::helper;\n\nfn renamed() {}\nfn run() { renamed(); }\n",
            )
            .expect("write");
        },
        &temp,
    );
}

#[test]
fn update_equals_fresh_for_a_call_added() {
    let temp = TempDir::new("t3c-eq-addcall");
    let root = candidate_repo(&temp, "repo");
    assert_update_equals_fresh(
        "addcall",
        &root,
        |root| {
            let path = root.join("src/lib.rs");
            let mut source = std::fs::read_to_string(&path).expect("read");
            source.push_str("fn again() { helper(); }\n");
            std::fs::write(path, source).expect("write");
        },
        &temp,
    );
}

#[test]
fn update_equals_fresh_for_a_call_removed() {
    let temp = TempDir::new("t3c-eq-delcall");
    let root = candidate_repo(&temp, "repo");
    assert_update_equals_fresh(
        "delcall",
        &root,
        |root| {
            std::fs::write(
                root.join("src/lib.rs"),
                b"mod util;\n\nuse crate::util::helper;\n\nfn helper() {}\nfn run() {}\n",
            )
            .expect("write");
        },
        &temp,
    );
}

#[test]
fn update_equals_fresh_for_a_module_changed() {
    let temp = TempDir::new("t3c-eq-mod");
    let root = candidate_repo(&temp, "repo");
    assert_update_equals_fresh(
        "mod",
        &root,
        |root| {
            // Move `helper` into a nested module; `run`'s helper() loses its
            // candidate under the bounded rule.
            std::fs::write(
                root.join("src/lib.rs"),
                b"mod util;\n\nuse crate::util::helper;\n\nmod inner {\n    fn helper() {}\n}\nfn run() { helper(); }\n",
            )
            .expect("write");
        },
        &temp,
    );
}

// ---------------------------------------------------------------------------
// Verification and integrity
// ---------------------------------------------------------------------------

#[test]
fn a_fresh_artifact_verifies() {
    let temp = TempDir::new("t3c-verify");
    let (snap, links, index) = fixture_candidates(&temp);
    let candidates_dir = store(&temp, "candidates-again");
    candidate(&snap, &links, &candidates_dir);
    let report = verify(&candidates_dir, &snap, &links).expect("verify");
    assert_eq!(report.records, index.records().len() as u64);
    assert_eq!(report.snapshot_digest, index.manifest.snapshot_digest);
    assert_eq!(report.link_digest, index.manifest.link_digest);
}

#[test]
fn a_stale_snapshot_dependency_is_rejected() {
    let temp = TempDir::new("t3c-stale-snap");
    let root = fixture_root();
    let snapshot_dir = store(&temp, "snap");
    snapshot(&root, &snapshot_dir);
    let links_dir = store(&temp, "links");
    link(&snapshot_dir, &root, &links_dir);
    let candidates_dir = store(&temp, "candidates");
    candidate(&snapshot_dir, &links_dir, &candidates_dir);

    // A different snapshot must not verify against this artifact.
    let other_root = candidate_repo(&temp, "other");
    let other_snapshot = store(&temp, "other-snap");
    snapshot(&other_root, &other_snapshot);
    let error = verify(&candidates_dir, &other_snapshot, &links_dir).expect_err("must fail");
    assert!(matches!(error, CandidateError::SnapshotMismatch { .. }));
}

#[test]
fn a_stale_link_dependency_is_rejected() {
    let temp = TempDir::new("t3c-stale-link");
    let root = fixture_root();
    let snapshot_dir = store(&temp, "snap");
    snapshot(&root, &snapshot_dir);
    let links_dir = store(&temp, "links");
    link(&snapshot_dir, &root, &links_dir);
    let candidates_dir = store(&temp, "candidates");
    candidate(&snapshot_dir, &links_dir, &candidates_dir);

    // A link artifact whose digest differs from the recorded dependency must
    // not verify. Tamper the link manifest's own digest to a different
    // valid-format value; the snapshot dependency still matches.
    let tampered = store(&temp, "links-tampered");
    copy_tree(&links_dir, &tampered);
    let mut manifest = repodex::links::read_manifest(&tampered).expect("load link manifest");
    manifest.link_digest = FOREIGN_DIGEST.to_string();
    std::fs::write(
        tampered.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest).expect("serialize"),
    )
    .expect("write");
    let error = verify(&candidates_dir, &snapshot_dir, &tampered).expect_err("must fail");
    assert!(matches!(error, CandidateError::LinkMismatch { .. }));
}

#[test]
fn a_tampered_record_is_rejected() {
    let temp = TempDir::new("t3c-tamper");
    let (snap, links, _index) = fixture_candidates(&temp);
    let candidates_dir = store(&temp, "candidates");
    candidate(&snap, &links, &candidates_dir);
    // Change one record's outcome without touching the digest.
    let mut records = read_records(&candidates_dir).expect("load");
    records[0].outcome = CandidateOutcome::no_candidate("tampered");
    rewrite_records(&candidates_dir, &records);
    let error = verify(&candidates_dir, &snap, &links).expect_err("must fail");
    assert!(matches!(
        error,
        CandidateError::CandidateDigestMismatch { .. }
    ));
}

#[test]
fn an_unsorted_record_list_is_rejected() {
    let temp = TempDir::new("t3c-unsorted");
    let (snap, links, _index) = fixture_candidates(&temp);
    let candidates_dir = store(&temp, "candidates");
    candidate(&snap, &links, &candidates_dir);
    let mut records = read_records(&candidates_dir).expect("load");
    records.reverse();
    rewrite_records(&candidates_dir, &records);
    // Recompute the digest so only ordering is wrong.
    mutate_candidate_manifest(&candidates_dir, |_| {});
    let error = verify(&candidates_dir, &snap, &links).expect_err("must fail");
    assert!(matches!(error, CandidateError::UnsortedRecords));
}

#[test]
fn a_candidate_pointing_at_a_non_function_is_rejected() {
    let temp = TempDir::new("t3c-forbidden");
    let (snap, links, _index) = fixture_candidates(&temp);
    let candidates_dir = store(&temp, "candidates");
    candidate(&snap, &links, &candidates_dir);
    // Point a record's candidate at the `struct User` declaration in scope.rs.
    let mut records = read_records(&candidates_dir).expect("load");
    let record = records
        .iter_mut()
        .find(|r| r.source.relative_path == "src/lib.rs" && r.written == "helper")
        .expect("record");
    record.outcome = CandidateOutcome::SingleCandidate {
        candidate: repodex::candidates::CandidateTarget::new(
            "src/scope.rs",
            1, // `struct User`, a struct not a function
            "struct",
            "User",
            "file",
        ),
    };
    rewrite_records(&candidates_dir, &records);
    mutate_candidate_manifest(&candidates_dir, |_| {});
    let error = verify(&candidates_dir, &snap, &links).expect_err("must fail");
    assert!(matches!(
        error,
        CandidateError::ForbiddenCandidateKind { .. }
    ));
}

#[test]
fn a_cardinality_that_lies_is_rejected() {
    let temp = TempDir::new("t3c-cardinality");
    let (snap, links, _index) = fixture_candidates(&temp);
    let candidates_dir = store(&temp, "candidates");
    candidate(&snap, &links, &candidates_dir);
    // Label a SingleCandidate as MultipleCandidates while keeping one candidate.
    let mut records = read_records(&candidates_dir).expect("load");
    let record = records
        .iter_mut()
        .find(|r| matches!(r.outcome, CandidateOutcome::SingleCandidate { .. }))
        .expect("record");
    if let CandidateOutcome::SingleCandidate { candidate } = &record.outcome {
        record.outcome = CandidateOutcome::MultipleCandidates {
            candidates: vec![candidate.clone()],
        };
    }
    rewrite_records(&candidates_dir, &records);
    mutate_candidate_manifest(&candidates_dir, |_| {});
    let error = verify(&candidates_dir, &snap, &links).expect_err("must fail");
    assert!(matches!(error, CandidateError::CardinalityMismatch { .. }));
}

#[test]
fn a_missing_call_locator_is_rejected() {
    let temp = TempDir::new("t3c-missing-loc");
    let (snap, links, _index) = fixture_candidates(&temp);
    let candidates_dir = store(&temp, "candidates");
    candidate(&snap, &links, &candidates_dir);
    let mut records = read_records(&candidates_dir).expect("load");
    records[0].source.fact_id = 999_999;
    records[0].source.fact_kind = FactKind::Call;
    rewrite_records(&candidates_dir, &records);
    mutate_candidate_manifest(&candidates_dir, |_| {});
    let error = verify(&candidates_dir, &snap, &links).expect_err("must fail");
    assert!(matches!(error, CandidateError::MissingLocator { .. }));
}
