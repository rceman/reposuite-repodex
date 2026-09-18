//! The derived cross-file structural relationship model.
//!
//! Everything here is **derived from** a TASK 3A `RepositoryFactSnapshot` plus
//! narrowly scoped repository metadata. Nothing here is semantic resolution:
//! an [`LinkOutcome::Exact`] says "under this documented structural rule there is
//! exactly one structural candidate", never "this is the runtime target".
//!
//! Two properties are load-bearing:
//!
//! * every relationship carries reproducible provenance (rule id, source
//!   occurrence locator, written form, evidence), so a consumer can always ask
//!   *why* RepoDex believes two things are related;
//! * ambiguity survives as ambiguity. There is no confidence score, no
//!   first-candidate fallback and no guessed target.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::repository::digest;

/// Version of the derived link record schema.
pub const LINK_SCHEMA_VERSION: u32 = 1;
/// Version of the link artifact manifest format.
pub const LINK_MANIFEST_VERSION: u32 = 1;

/// Version of the link *rule* semantics.
///
/// **Increment whenever a change to any link rule could change the relationship
/// set produced from an unchanged snapshot** — a new rule, a changed candidate
/// construction, a changed Exact/Ambiguous/Unresolved boundary, or a changed
/// resolution policy. It does **not** need to change for documentation, tests,
/// CLI formatting or performance work.
///
/// This version participates in [`LinkFingerprint`], so bumping it invalidates
/// previously derived link artifacts without touching the TASK 3A snapshot.
pub const LINK_RULE_ABI_VERSION: u32 = 1;

/// Per-language resolution-policy versions.
///
/// These are separate from the ABI version so that a change confined to one
/// language's policy is visible as such in the fingerprint text.
pub const POLICY_VERSION_RUST: u32 = 1;
pub const POLICY_VERSION_GO: u32 = 1;
pub const POLICY_VERSION_PYTHON: u32 = 1;
pub const POLICY_VERSION_PHP: u32 = 1;

/// Stable, machine-readable link rule identifiers.
///
/// These strings appear in every link record and in the rule registry, so they
/// must never be renamed once shipped.
pub mod rule {
    pub const RUST_MOD_STANDARD_FILE: &str = "rust.mod.standard_file";
    pub const RUST_USE_CRATE_PATH: &str = "rust.use.crate_path";
    pub const RUST_USE_NON_CRATE_PATH: &str = "rust.use.non_crate_path";

    pub const GO_PACKAGE_SAME_DIRECTORY: &str = "go.package.same_directory";
    pub const GO_IMPORT_LOCAL_MODULE: &str = "go.import.local_module";
    pub const GO_IMPORT_EXTERNAL: &str = "go.import.external";

    pub const PYTHON_RELATIVE_IMPORT_PACKAGE_PATH: &str = "python.relative_import.package_path";
    pub const PYTHON_ABSOLUTE_IMPORT_LOCAL_CANDIDATE: &str =
        "python.absolute_import.local_candidate";
    pub const PYTHON_ABSOLUTE_IMPORT_EXTERNAL: &str = "python.absolute_import.external";

    pub const PHP_NAMESPACE_DECLARATION: &str = "php.namespace.declaration";
    pub const PHP_USE_QUALIFIED_NAME: &str = "php.use.qualified_name";
    pub const PHP_USE_EXTERNAL: &str = "php.use.external";
    pub const PHP_USE_NON_CLASS_IMPORT: &str = "php.use.non_class_import";

    /// Every rule this build can emit, in canonical order.
    pub const ALL: &[&str] = &[
        RUST_MOD_STANDARD_FILE,
        RUST_USE_CRATE_PATH,
        RUST_USE_NON_CRATE_PATH,
        GO_PACKAGE_SAME_DIRECTORY,
        GO_IMPORT_LOCAL_MODULE,
        GO_IMPORT_EXTERNAL,
        PYTHON_RELATIVE_IMPORT_PACKAGE_PATH,
        PYTHON_ABSOLUTE_IMPORT_LOCAL_CANDIDATE,
        PYTHON_ABSOLUTE_IMPORT_EXTERNAL,
        PHP_NAMESPACE_DECLARATION,
        PHP_USE_QUALIFIED_NAME,
        PHP_USE_EXTERNAL,
        PHP_USE_NON_CLASS_IMPORT,
    ];
}

/// Which kind of normalized fact a locator points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FactKind {
    Declaration,
    Import,
    Reference,
    Call,
}

impl FactKind {
    pub fn as_str(self) -> &'static str {
        match self {
            FactKind::Declaration => "declaration",
            FactKind::Import => "import",
            FactKind::Reference => "reference",
            FactKind::Call => "call",
        }
    }
}

/// A snapshot-local locator for one normalized fact.
///
/// This deliberately reuses TASK 3A's file-local fact ids instead of inventing a
/// global semantic symbol id. File-local ids are **not** promised to be stable
/// across source edits, and nothing here claims otherwise.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct FactLocator {
    /// `/`-separated path relative to the repository root.
    pub relative_path: String,
    pub fact_kind: FactKind,
    /// Index of the fact inside its file analysis.
    pub fact_id: u32,
    /// Index of the entry inside a multi-entry import statement, when the
    /// relationship is attached to one entry rather than the whole statement.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_index: Option<u32>,
}

impl FactLocator {
    pub fn declaration(relative_path: impl Into<String>, declaration_id: u32) -> Self {
        Self {
            relative_path: relative_path.into(),
            fact_kind: FactKind::Declaration,
            fact_id: declaration_id,
            item_index: None,
        }
    }

    pub fn import(relative_path: impl Into<String>, import_id: u32) -> Self {
        Self {
            relative_path: relative_path.into(),
            fact_kind: FactKind::Import,
            fact_id: import_id,
            item_index: None,
        }
    }

    /// Compact deterministic key, used for canonical text and derived ids.
    pub fn key(&self) -> String {
        match self.item_index {
            Some(item) => format!(
                "{}#{}:{}+{}",
                self.relative_path,
                self.fact_kind.as_str(),
                self.fact_id,
                item
            ),
            None => format!(
                "{}#{}:{}",
                self.relative_path,
                self.fact_kind.as_str(),
                self.fact_id
            ),
        }
    }
}

/// What a relationship points at.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LinkTarget {
    /// A source file that is part of the snapshot.
    File { relative_path: String },
    /// One normalized declaration, identified snapshot-locally.
    Declaration {
        relative_path: String,
        declaration_id: u32,
        /// Structural qualified name under the rule's documented assumptions.
        /// For PHP this is the syntactic namespace-qualified name; it is not
        /// proof of autoload or runtime availability.
        qualified_name: String,
        declaration_kind: String,
        name: String,
    },
    /// A derived structural entity such as a Go package group or a PHP
    /// namespace. Identified by the entity's derived id.
    Structure {
        entity_id: String,
        structural_kind: String,
        key: String,
    },
}

impl LinkTarget {
    pub fn file(relative_path: impl Into<String>) -> Self {
        LinkTarget::File {
            relative_path: relative_path.into(),
        }
    }

    pub fn declaration(
        relative_path: impl Into<String>,
        declaration_id: u32,
        qualified_name: impl Into<String>,
        declaration_kind: impl Into<String>,
        name: impl Into<String>,
    ) -> Self {
        LinkTarget::Declaration {
            relative_path: relative_path.into(),
            declaration_id,
            qualified_name: qualified_name.into(),
            declaration_kind: declaration_kind.into(),
            name: name.into(),
        }
    }

    /// Deterministic canonical rendering.
    pub fn render(&self) -> String {
        match self {
            LinkTarget::File { relative_path } => format!("file:{relative_path}"),
            LinkTarget::Declaration {
                relative_path,
                declaration_id,
                qualified_name,
                ..
            } => format!("decl:{relative_path}#{declaration_id}:{qualified_name}"),
            LinkTarget::Structure {
                entity_id,
                structural_kind,
                key,
            } => format!("struct:{structural_kind}:{entity_id}:{key}"),
        }
    }

    /// The file this target lives in, when it has one.
    pub fn relative_path(&self) -> &str {
        match self {
            LinkTarget::File { relative_path } => relative_path,
            LinkTarget::Declaration { relative_path, .. } => relative_path,
            LinkTarget::Structure { key, .. } => key,
        }
    }

    /// Sort key, so candidate lists have a deterministic order.
    ///
    /// This deliberately sorts by the *meaningful* key — path, declaration id,
    /// structural kind and structural key — rather than by the rendered form,
    /// which embeds a derived entity id. Sorting by a hash would still be
    /// deterministic but would make candidate lists unreadable.
    pub fn canonical_key(&self) -> String {
        match self {
            LinkTarget::File { relative_path } => format!("0:file:{relative_path}"),
            LinkTarget::Declaration {
                relative_path,
                declaration_id,
                qualified_name,
                ..
            } => format!("1:decl:{relative_path}#{declaration_id}:{qualified_name}"),
            LinkTarget::Structure {
                structural_kind,
                key,
                ..
            } => format!("2:struct:{structural_kind}:{key}"),
        }
    }
}

/// Put a candidate list into canonical order and drop exact duplicates.
///
/// This is the single implementation of deterministic candidate ordering, so
/// every outcome that carries candidates shares it.
pub fn sort_candidates(candidates: &mut Vec<LinkTarget>) {
    candidates.sort_by_key(LinkTarget::canonical_key);
    candidates.dedup_by(|left, right| left.canonical_key() == right.canonical_key());
}

/// The discrete outcome of one relationship.
///
/// There is deliberately no numeric confidence. A discrete outcome plus
/// explicit evidence is easier to audit than a score.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum LinkOutcome {
    /// Exactly one structural candidate, under this rule's documented
    /// assumptions. Never "the runtime target".
    Exact { target: LinkTarget },
    /// More than one structural candidate. Ambiguity is preserved, never
    /// collapsed to the first candidate.
    Ambiguous { candidates: Vec<LinkTarget> },
    /// The rule applied but produced no candidate, or the needed structure is
    /// missing from the snapshot.
    Unresolved { reason: String },
    /// The relationship is deliberately outside this task's scope.
    OutOfScope { reason: String },
}

impl LinkOutcome {
    pub fn exact(target: LinkTarget) -> Self {
        LinkOutcome::Exact { target }
    }

    pub fn unresolved(reason: impl Into<String>) -> Self {
        LinkOutcome::Unresolved {
            reason: reason.into(),
        }
    }

    pub fn out_of_scope(reason: impl Into<String>) -> Self {
        LinkOutcome::OutOfScope {
            reason: reason.into(),
        }
    }

    /// Build an outcome from a candidate list.
    ///
    /// This is the **only** place a candidate list becomes an outcome, so the
    /// "never collapse ambiguity" rule has a single implementation: zero
    /// candidates is `Unresolved`, one is `Exact`, and two or more is
    /// `Ambiguous`.
    pub fn from_candidates(
        mut candidates: Vec<LinkTarget>,
        empty_reason: impl Into<String>,
    ) -> Self {
        sort_candidates(&mut candidates);
        match candidates.len() {
            0 => LinkOutcome::unresolved(empty_reason),
            1 => LinkOutcome::exact(candidates.remove(0)),
            _ => LinkOutcome::Ambiguous { candidates },
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            LinkOutcome::Exact { .. } => "exact",
            LinkOutcome::Ambiguous { .. } => "ambiguous",
            LinkOutcome::Unresolved { .. } => "unresolved",
            LinkOutcome::OutOfScope { .. } => "out_of_scope",
        }
    }

    /// The single target of an `Exact` outcome.
    pub fn target(&self) -> Option<&LinkTarget> {
        match self {
            LinkOutcome::Exact { target } => Some(target),
            _ => None,
        }
    }

    /// The candidate list of an `Ambiguous` outcome.
    pub fn candidates(&self) -> &[LinkTarget] {
        match self {
            LinkOutcome::Ambiguous { candidates } => candidates,
            _ => &[],
        }
    }

    /// Every target this outcome names, exact or candidate.
    pub fn all_targets(&self) -> Vec<&LinkTarget> {
        match self {
            LinkOutcome::Exact { target } => vec![target],
            LinkOutcome::Ambiguous { candidates } => candidates.iter().collect(),
            LinkOutcome::Unresolved { .. } | LinkOutcome::OutOfScope { .. } => Vec::new(),
        }
    }

    /// Deterministic canonical rendering.
    pub fn render(&self) -> String {
        match self {
            LinkOutcome::Exact { target } => format!("exact({})", target.render()),
            LinkOutcome::Ambiguous { candidates } => {
                let rendered = candidates
                    .iter()
                    .map(LinkTarget::render)
                    .collect::<Vec<_>>()
                    .join(",");
                format!("ambiguous([{rendered}])")
            }
            LinkOutcome::Unresolved { reason } => format!("unresolved({reason})"),
            LinkOutcome::OutOfScope { reason } => format!("out_of_scope({reason})"),
        }
    }
}

/// A repository metadata file a rule depended on.
///
/// Recorded by content digest, not by mtime, so a derived link artifact becomes
/// invalid when the metadata's *content* changes.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct MetadataDependency {
    pub relative_path: String,
    pub content_digest: String,
    /// False when the rule depends on the *absence* of this file.
    ///
    /// This is recorded rather than omitted so that adding a `go.mod` where none
    /// existed invalidates the derived artifact, exactly as changing one does.
    #[serde(default = "default_true")]
    pub present: bool,
    /// The rule that consumed this metadata.
    pub rule_id: String,
    /// The field that was read.
    pub field: String,
    /// The value that was read, so the dependency is auditable without
    /// re-reading the file.
    pub value: String,
}

fn default_true() -> bool {
    true
}

impl MetadataDependency {
    fn render(&self) -> String {
        format!(
            "dep rule={} path={} present={} digest={} field={} value={}",
            self.rule_id,
            escape(&self.relative_path),
            self.present,
            self.content_digest,
            escape(&self.field),
            escape(&self.value),
        )
    }
}

/// Provenance for one relationship.
///
/// The rule's documented conditions and assumptions live once in the rule
/// registry (see [`RuleDocumentation`]); this record references the rule by id
/// and adds the occurrence-specific evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkProvenance {
    pub language: String,
    pub rule_id: String,
    /// The source occurrence the relationship was derived from.
    pub source: FactLocator,
    /// The written source form, exactly as it appears in the source.
    pub written: String,
    /// Rule-specific evidence, deterministic order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence: Vec<String>,
    /// Metadata this particular relationship depended on.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub metadata: Vec<MetadataDependency>,
}

impl LinkProvenance {
    fn render(&self) -> String {
        let evidence = self.evidence.join("|");
        let metadata = self
            .metadata
            .iter()
            .map(MetadataDependency::render)
            .collect::<Vec<_>>()
            .join("|");
        format!(
            "prov lang={} rule={} source={} written={} evidence={} metadata={}",
            self.language,
            self.rule_id,
            self.source.key(),
            escape(&self.written),
            escape(&evidence),
            escape(&metadata),
        )
    }
}

/// One derived cross-file relationship.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkRecord {
    /// Content-derived id, stable for a given snapshot and rule set.
    pub link_id: String,
    pub kind: String,
    pub rule_id: String,
    pub language: String,
    pub source: FactLocator,
    pub written: String,
    pub outcome: LinkOutcome,
    pub provenance: LinkProvenance,
}

impl LinkRecord {
    /// Build a record, deriving its id from the relationship's own content.
    pub fn new(
        kind: &str,
        source: FactLocator,
        written: impl Into<String>,
        outcome: LinkOutcome,
        provenance: LinkProvenance,
    ) -> Self {
        let written = written.into();
        let link_id = link_id_for(&provenance.rule_id, &source, &written);
        Self {
            link_id,
            kind: kind.to_string(),
            rule_id: provenance.rule_id.clone(),
            language: provenance.language.clone(),
            source,
            written,
            outcome,
            provenance,
        }
    }

    /// Canonical one-line rendering, used by the link digest.
    fn render(&self) -> String {
        format!(
            "link id={} kind={} rule={} lang={} outcome={} {}",
            self.link_id,
            self.kind,
            self.rule_id,
            self.language,
            self.outcome.render(),
            self.provenance.render(),
        )
    }
}

/// Derive a link id from the relationship's own content.
///
/// Including the source locator keeps two identical relationships from two
/// different occurrences distinct, so a link id is unique per source
/// occurrence rather than per written form.
pub fn link_id_for(rule_id: &str, source: &FactLocator, written: &str) -> String {
    let seed = format!("repodex-link-v1\n{rule_id}\n{}\n{written}", source.key());
    let digest = digest::sha256_text(&seed);
    let hex = digest.strip_prefix("sha256:").unwrap_or(&digest);
    format!("lnk-{}", &hex[..16])
}

/// A derived structural identity.
///
/// These are **not** universal runtime semantic identities. Each one is
/// derivable from written syntax plus repository organization, and each carries
/// the assumptions that make it valid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StructuralEntity {
    pub entity_id: String,
    /// `go_package`, `php_namespace`, `python_package`, `rust_module`.
    pub structural_kind: String,
    pub language: String,
    /// The structural key, e.g. `internal/foo:foo` for a Go package.
    pub key: String,
    /// Files that define this entity, in canonical path order.
    pub files: Vec<String>,
    /// Assumptions this identity rests on.
    pub assumptions: Vec<String>,
}

impl StructuralEntity {
    /// Derive an entity id from its kind and key.
    pub fn derive_id(structural_kind: &str, key: &str) -> String {
        let seed = format!("repodex-structure-v1\n{structural_kind}\n{key}");
        let digest = digest::sha256_text(&seed);
        let hex = digest.strip_prefix("sha256:").unwrap_or(&digest);
        format!("ent-{}", &hex[..16])
    }

    fn render(&self) -> String {
        format!(
            "entity id={} kind={} lang={} key={} files=[{}]",
            self.entity_id,
            self.structural_kind,
            self.language,
            escape(&self.key),
            self.files
                .iter()
                .map(|file| escape(file))
                .collect::<Vec<_>>()
                .join(","),
        )
    }
}

/// Outcome counters over a whole link artifact.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutcomeCounts {
    pub exact: u64,
    pub ambiguous: u64,
    pub unresolved: u64,
    pub out_of_scope: u64,
}

impl OutcomeCounts {
    pub fn record(&mut self, outcome: &LinkOutcome) {
        match outcome {
            LinkOutcome::Exact { .. } => self.exact += 1,
            LinkOutcome::Ambiguous { .. } => self.ambiguous += 1,
            LinkOutcome::Unresolved { .. } => self.unresolved += 1,
            LinkOutcome::OutOfScope { .. } => self.out_of_scope += 1,
        }
    }

    pub fn total(&self) -> u64 {
        self.exact + self.ambiguous + self.unresolved + self.out_of_scope
    }
}

/// Documentation of one rule, stored once in the artifact.
///
/// Every rule documents its input syntax, the repository assumptions it needs,
/// its metadata dependency, and the exact conditions under which it produces
/// each outcome.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleDocumentation {
    pub rule_id: String,
    pub kind: String,
    pub language: String,
    pub summary: String,
    /// Input syntax the rule matches.
    pub input_syntax: String,
    /// Repository assumptions the rule needs.
    pub repository_assumptions: Vec<String>,
    /// Metadata files the rule may read.
    pub metadata_dependency: Vec<String>,
    /// Condition for `Exact`.
    pub exact_condition: String,
    /// Condition for `Ambiguous`.
    pub ambiguous_condition: String,
    /// Condition for `Unresolved`.
    pub unresolved_condition: String,
    /// Condition for `OutOfScope`.
    pub out_of_scope_condition: String,
    /// How candidates are constructed.
    pub candidate_rule: String,
    /// Known exclusions.
    pub known_exclusions: Vec<String>,
}

/// Identity of the link rule set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkFingerprint {
    pub text: String,
    pub digest: String,
}

impl LinkFingerprint {
    /// Compute the fingerprint of the link rule set this build implements.
    pub fn current() -> Self {
        let mut text = format!(
            "link_schema={LINK_SCHEMA_VERSION} link_manifest={LINK_MANIFEST_VERSION} \
             rule_abi={LINK_RULE_ABI_VERSION} \
             rust={POLICY_VERSION_RUST} go={POLICY_VERSION_GO} \
             python={POLICY_VERSION_PYTHON} php={POLICY_VERSION_PHP}"
        );
        for rule_id in rule::ALL {
            text.push_str(&format!(" rule={rule_id}"));
        }
        let digest = digest::sha256_text(&text);
        Self { text, digest }
    }
}

/// The link artifact manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkManifest {
    pub link_manifest_version: u32,
    pub link_schema_version: u32,
    pub link_rule_abi_version: u32,

    /// The exact TASK 3A snapshot this artifact was derived from.
    pub snapshot_digest: String,
    pub snapshot_schema_version: u32,
    pub snapshot_analyzer_fingerprint: String,

    /// Identity of the link rule set.
    pub link_fingerprint: String,
    #[serde(default)]
    pub link_fingerprint_text: String,

    /// Digest over canonical link content.
    pub link_digest: String,

    pub links: u64,
    pub structural_entities: u64,
    pub outcomes: OutcomeCounts,
    /// Link counts by kind, in canonical key order.
    pub kinds: BTreeMap<String, u64>,
    /// Rule registry: every rule, documented once.
    pub rules: Vec<RuleDocumentation>,
    /// Every metadata file the derivation depended on.
    pub metadata: Vec<MetadataDependency>,
}

impl LinkManifest {
    /// Canonical text over everything the link digest covers.
    ///
    /// Excludes the free-text fingerprint, the link digest itself and all
    /// operational counters that are derived from the links, so the digest is a
    /// pure function of the dependency identity plus the link and entity
    /// content.
    pub fn canonical_text(&self) -> String {
        let mut text = String::new();
        text.push_str(&format!(
            "links manifest_version={} schema={} rule_abi={} snapshot={} snapshot_schema={} \
             snapshot_fingerprint={} link_fingerprint={}\n",
            self.link_manifest_version,
            self.link_schema_version,
            self.link_rule_abi_version,
            self.snapshot_digest,
            self.snapshot_schema_version,
            self.snapshot_analyzer_fingerprint,
            self.link_fingerprint,
        ));
        for dependency in &self.metadata {
            text.push_str(&dependency.render());
            text.push('\n');
        }
        text
    }

    /// Recompute the link digest from canonical content plus the link records.
    pub fn compute_link_digest(
        &self,
        links: &[LinkRecord],
        entities: &[StructuralEntity],
    ) -> String {
        let mut text = self.canonical_text();
        for entity in entities {
            text.push_str(&entity.render());
            text.push('\n');
        }
        for link in links {
            text.push_str(&link.render());
            text.push('\n');
        }
        digest::sha256_text(&text)
    }

    pub fn refresh_link_digest(&mut self, links: &[LinkRecord], entities: &[StructuralEntity]) {
        self.link_digest = self.compute_link_digest(links, entities);
    }

    /// The documentation of one rule.
    pub fn rule(&self, rule_id: &str) -> Option<&RuleDocumentation> {
        self.rules.iter().find(|rule| rule.rule_id == rule_id)
    }
}

/// Canonical facts are one relationship per line, so free text must never
/// contain a raw newline. Backslashes are escaped first so the mapping stays
/// injective.
pub(crate) fn escape(value: &str) -> String {
    if !value.contains(['\\', '\n', '\r', '\t']) {
        return value.to_string();
    }
    let mut escaped = String::with_capacity(value.len() + 8);
    for character in value.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            other => escaped.push(other),
        }
    }
    escaped
}

/// Every rule's documentation, in canonical rule-id order.
pub fn rule_registry() -> Vec<RuleDocumentation> {
    use rule::*;
    vec![
        RuleDocumentation {
            rule_id: RUST_MOD_STANDARD_FILE.to_string(),
            kind: "module_file".to_string(),
            language: "rust".to_string(),
            summary: "External `mod name;` declaration mapped to a source file.".to_string(),
            input_syntax: "mod <name>;  (a Module declaration with no inline body)".to_string(),
            repository_assumptions: vec![
                "the crate uses the standard Rust 2018+ file layout".to_string(),
                "the child directory of a module file is <dir>/<stem>, except for mod.rs, \
                 lib.rs and main.rs which keep their own directory"
                    .to_string(),
                "`#[path = \"...\"]` attributes are not evaluated".to_string(),
            ],
            metadata_dependency: vec![],
            exact_condition: "exactly one of <dir>/<name>.rs or <dir>/<name>/mod.rs is present \
                              in the snapshot"
                .to_string(),
            ambiguous_condition:
                "both <dir>/<name>.rs and <dir>/<name>/mod.rs are present in the snapshot"
                    .to_string(),
            unresolved_condition: "neither candidate file is present in the snapshot".to_string(),
            out_of_scope_condition: "never; an external module declaration always produces a \
                                     candidate search"
                .to_string(),
            candidate_rule: "<dir>/<name>.rs and <dir>/<name>/mod.rs, where <dir> is derived from \
                             the declaring file's path plus the enclosing inline module scopes"
                .to_string(),
            known_exclusions: vec![
                "cfg-gated modules".to_string(),
                "macro-generated modules".to_string(),
                "`#[path]`-redirected modules".to_string(),
            ],
        },
        RuleDocumentation {
            rule_id: RUST_USE_CRATE_PATH.to_string(),
            kind: "use_path".to_string(),
            language: "rust".to_string(),
            summary: "`use crate::...` path resolved against the structural module tree."
                .to_string(),
            input_syntax: "use crate::a::b::C; / use crate::a::{b, c}; / use crate::a::*;"
                .to_string(),
            repository_assumptions: vec![
                "crate roots are files named lib.rs or main.rs".to_string(),
                "a file belongs to the crate root with the longest matching directory prefix"
                    .to_string(),
                "the module tree is built from `mod` declarations only".to_string(),
            ],
            metadata_dependency: vec![],
            exact_condition: "the whole module prefix resolves to one module and the final \
                              segment names exactly one declaration or child module in it"
                .to_string(),
            ambiguous_condition: "the module prefix resolves but the final segment names more \
                                  than one declaration, or a prefix step has more than one \
                                  candidate module"
                .to_string(),
            unresolved_condition: "no crate root was identified, a prefix step matched no module, \
                                   or the final segment matched nothing"
                .to_string(),
            out_of_scope_condition: "never; paths that do not begin with `crate` are handled by \
                                     rust.use.non_crate_path"
                .to_string(),
            candidate_rule: "walk the written segments from the crate root module; each \
                             non-final segment must be a child module reachable from `mod` \
                             declarations; the final segment is matched against declarations and \
                             child modules of the reached module"
                .to_string(),
            known_exclusions: vec![
                "glob imports (`use crate::a::*` produces a module-level candidate, not the \
                 imported names)"
                    .to_string(),
                "re-exports (`pub use`)".to_string(),
                "prelude".to_string(),
                "macro-generated names".to_string(),
                "trait lookup".to_string(),
                "`self::` and `super::` paths".to_string(),
            ],
        },
        RuleDocumentation {
            rule_id: RUST_USE_NON_CRATE_PATH.to_string(),
            kind: "use_path".to_string(),
            language: "rust".to_string(),
            summary: "A `use` path that does not begin with `crate`, recorded as out of scope \
                      rather than silently dropped."
                .to_string(),
            input_syntax: "use self::... / use super::... / use std::... / use <extern>::..."
                .to_string(),
            repository_assumptions: vec![
                "only `crate::` paths have a structural meaning derivable from this snapshot"
                    .to_string(),
            ],
            metadata_dependency: vec![],
            exact_condition: "never".to_string(),
            ambiguous_condition: "never".to_string(),
            unresolved_condition: "never".to_string(),
            out_of_scope_condition: "always; the leading segment is not `crate`".to_string(),
            candidate_rule: "no candidates".to_string(),
            known_exclusions: vec![
                "`self::` paths".to_string(),
                "`super::` paths".to_string(),
                "extern crates and the standard library".to_string(),
                "the prelude".to_string(),
            ],
        },
        RuleDocumentation {
            rule_id: GO_PACKAGE_SAME_DIRECTORY.to_string(),
            kind: "package_membership".to_string(),
            language: "go".to_string(),
            summary: "Files in one directory declaring the same package name form one \
                      structural package group."
                .to_string(),
            input_syntax: "package <name>".to_string(),
            repository_assumptions: vec![
                "membership is structural over indexed files, not a particular Go build \
                 configuration"
                    .to_string(),
                "build tags and file-name constraints are not evaluated".to_string(),
            ],
            metadata_dependency: vec![],
            exact_condition: "every indexed file in the directory declares the same package name"
                .to_string(),
            ambiguous_condition: "the directory contains more than one distinct declared package \
                                  name (each name still forms its own group)"
                .to_string(),
            unresolved_condition: "the directory contains no indexed Go file".to_string(),
            out_of_scope_condition: "never".to_string(),
            candidate_rule: "group indexed Go files by (directory, declared package name)"
                .to_string(),
            known_exclusions: vec![
                "build tags".to_string(),
                "`_test` package suffixes are kept as distinct groups".to_string(),
            ],
        },
        RuleDocumentation {
            rule_id: GO_IMPORT_LOCAL_MODULE.to_string(),
            kind: "import_path".to_string(),
            language: "go".to_string(),
            summary: "Go import whose path is inside the repository's own module.".to_string(),
            input_syntax: "import \"<module-path>/...\"".to_string(),
            repository_assumptions: vec![
                "a go.mod at the repository root declares the module path".to_string(),
                "the import path maps directly onto a repository directory".to_string(),
            ],
            metadata_dependency: vec!["go.mod (required; the `module` directive)".to_string()],
            exact_condition: "the mapped directory exists and contains exactly one distinct \
                              declared package name"
                .to_string(),
            ambiguous_condition: "the mapped directory exists and contains more than one distinct \
                                  declared package name"
                .to_string(),
            unresolved_condition: "the import path is inside the module prefix but the mapped \
                                   directory contains no indexed Go file"
                .to_string(),
            out_of_scope_condition: "never; a path outside the module prefix is handled by \
                                     go.import.external"
                .to_string(),
            candidate_rule: "strip the module prefix from the import path and map the remainder \
                             onto a repository directory; candidates are the package groups in \
                             that directory"
                .to_string(),
            known_exclusions: vec![
                "replace directives".to_string(),
                "vendored dependencies".to_string(),
                "workspace files".to_string(),
            ],
        },
        RuleDocumentation {
            rule_id: GO_IMPORT_EXTERNAL.to_string(),
            kind: "import_path".to_string(),
            language: "go".to_string(),
            summary: "Go import outside the repository's own module.".to_string(),
            input_syntax: "import \"<path>\"".to_string(),
            repository_assumptions: vec![
                "only the repository's own module is in scope for TASK 3B".to_string(),
            ],
            metadata_dependency: vec![
                "go.mod (optional; without it every import is external)".to_string()
            ],
            exact_condition: "never".to_string(),
            ambiguous_condition: "never".to_string(),
            unresolved_condition: "never".to_string(),
            out_of_scope_condition: "the import path is not inside the local module prefix, or no \
                                     go.mod was found"
                .to_string(),
            candidate_rule: "no candidates; external dependencies are never inspected".to_string(),
            known_exclusions: vec![
                "standard library".to_string(),
                "third-party modules".to_string(),
            ],
        },
        RuleDocumentation {
            rule_id: PYTHON_RELATIVE_IMPORT_PACKAGE_PATH.to_string(),
            kind: "module_import".to_string(),
            language: "python".to_string(),
            summary: "Explicit relative import resolved through the repository's package \
                      structure."
                .to_string(),
            input_syntax: "from .x import Y / from ..p import z / from . import q".to_string(),
            repository_assumptions: vec![
                "the repository root is the package root (policy P-PY-1)".to_string(),
                "a module path a.b.c is defined by a/b/c.py or a/b/c/__init__.py".to_string(),
                "only files present in the snapshot are candidates".to_string(),
            ],
            metadata_dependency: vec![],
            exact_condition: "exactly one candidate module file is present".to_string(),
            ambiguous_condition: "more than one candidate module file is present, or the \
                                  statement has no module part and both the package's \
                                  __init__.py and a submodule match"
                .to_string(),
            unresolved_condition: "no candidate module file is present, or the relative level \
                                   escapes above the repository root"
                .to_string(),
            out_of_scope_condition: "never; relative imports are always in scope".to_string(),
            candidate_rule: "resolve the base package from the importing file's directory minus \
                             (levels - 1) trailing segments, append the written module, and map \
                             the resulting dotted path to a.py or a/__init__.py"
                .to_string(),
            known_exclusions: vec![
                "namespace packages".to_string(),
                "`__path__` manipulation".to_string(),
                "editable installs".to_string(),
            ],
        },
        RuleDocumentation {
            rule_id: PYTHON_ABSOLUTE_IMPORT_LOCAL_CANDIDATE.to_string(),
            kind: "module_import".to_string(),
            language: "python".to_string(),
            summary: "Absolute Python import that matches at least one repository module. \
                      Candidates only, never Exact."
                .to_string(),
            input_syntax: "import x / from pkg import name".to_string(),
            repository_assumptions: vec![
                "a matching repository path is a candidate, not the resolved module".to_string(),
            ],
            metadata_dependency: vec![],
            exact_condition: "never; absolute resolution depends on sys.path, editable installs \
                              and namespace packages"
                .to_string(),
            ambiguous_condition: "at least one repository module file matches the written path"
                .to_string(),
            unresolved_condition: "never; a matchless absolute import is external".to_string(),
            out_of_scope_condition: "handled by python.absolute_import.external".to_string(),
            candidate_rule: "map the dotted written path to a.py or a/__init__.py under the \
                             repository root; every present file is a candidate"
                .to_string(),
            known_exclusions: vec![
                "sys.path ordering".to_string(),
                "installed distributions".to_string(),
                "namespace packages".to_string(),
            ],
        },
        RuleDocumentation {
            rule_id: PYTHON_ABSOLUTE_IMPORT_EXTERNAL.to_string(),
            kind: "module_import".to_string(),
            language: "python".to_string(),
            summary: "Absolute Python import with no repository module matching.".to_string(),
            input_syntax: "import x / from pkg import name".to_string(),
            repository_assumptions: vec![
                "only repository modules are in scope for TASK 3B".to_string()
            ],
            metadata_dependency: vec![],
            exact_condition: "never".to_string(),
            ambiguous_condition: "never".to_string(),
            unresolved_condition: "never".to_string(),
            out_of_scope_condition: "no repository module file matches the written path"
                .to_string(),
            candidate_rule: "no candidates".to_string(),
            known_exclusions: vec!["standard library".to_string(), "site-packages".to_string()],
        },
        RuleDocumentation {
            rule_id: PHP_NAMESPACE_DECLARATION.to_string(),
            kind: "namespace_membership".to_string(),
            language: "php".to_string(),
            summary: "Declarations grouped under their written namespace into syntactic \
                      qualified names."
                .to_string(),
            input_syntax: "namespace A\\B;  followed by declarations".to_string(),
            repository_assumptions: vec![
                "the syntactic qualified name is derived from written namespace syntax plus the \
                 written declaration name"
                    .to_string(),
            ],
            metadata_dependency: vec![],
            exact_condition: "every declaration in a namespace has a distinct qualified name"
                .to_string(),
            ambiguous_condition: "two declarations in the same file set produce the same \
                                  qualified name"
                .to_string(),
            unresolved_condition: "the file declares no namespace".to_string(),
            out_of_scope_condition: "never".to_string(),
            candidate_rule: "concatenate the enclosing namespace scope chain with the written \
                             declaration name, separated by `\\`"
                .to_string(),
            known_exclusions: vec![
                "autoload availability".to_string(),
                "runtime class existence".to_string(),
            ],
        },
        RuleDocumentation {
            rule_id: PHP_USE_QUALIFIED_NAME.to_string(),
            kind: "use_import".to_string(),
            language: "php".to_string(),
            summary: "`use A\\B\\C;` compared against syntactically qualified declarations."
                .to_string(),
            input_syntax: "use A\\B\\C; / use A\\B\\C as D;".to_string(),
            repository_assumptions: vec![
                "matching is textual against syntactic qualified names".to_string(),
                "an alias does not change the written imported target".to_string(),
            ],
            metadata_dependency: vec![],
            exact_condition: "exactly one declaration in the snapshot has the written qualified \
                              name"
                .to_string(),
            ambiguous_condition: "more than one declaration has the written qualified name"
                .to_string(),
            unresolved_condition: "no declaration matches, but the written name's namespace is \
                                   declared somewhere in the snapshot"
                .to_string(),
            out_of_scope_condition: "no declaration matches and no prefix of the written \
                                     namespace is declared (handled by php.use.external)"
                .to_string(),
            candidate_rule: "compare the written imported target textually against the syntactic \
                             qualified names of class-like declarations"
                .to_string(),
            known_exclusions: vec![
                "PSR-4 mapping".to_string(),
                "autoloading".to_string(),
                "class aliases at runtime".to_string(),
            ],
        },
        RuleDocumentation {
            rule_id: PHP_USE_EXTERNAL.to_string(),
            kind: "use_import".to_string(),
            language: "php".to_string(),
            summary: "`use` of a name whose namespace is not declared in the repository."
                .to_string(),
            input_syntax: "use A\\B\\C;".to_string(),
            repository_assumptions: vec![
                "only repository declarations are in scope for TASK 3B".to_string()
            ],
            metadata_dependency: vec![],
            exact_condition: "never".to_string(),
            ambiguous_condition: "never".to_string(),
            unresolved_condition: "never".to_string(),
            out_of_scope_condition: "no declaration matches and no prefix of the written \
                                     namespace is declared in the snapshot"
                .to_string(),
            candidate_rule: "no candidates; external dependencies are never inspected".to_string(),
            known_exclusions: vec!["vendor packages".to_string()],
        },
        RuleDocumentation {
            rule_id: PHP_USE_NON_CLASS_IMPORT.to_string(),
            kind: "use_import".to_string(),
            language: "php".to_string(),
            summary: "`use function` / `use const` imports, kept distinct from class imports."
                .to_string(),
            input_syntax: "use function A\\b; / use const A\\B;".to_string(),
            repository_assumptions: vec![
                "function and constant imports are not linked to class declarations".to_string(),
            ],
            metadata_dependency: vec![],
            exact_condition: "never in TASK 3B".to_string(),
            ambiguous_condition: "never in TASK 3B".to_string(),
            unresolved_condition: "never in TASK 3B".to_string(),
            out_of_scope_condition: "always; function and constant import linking is out of scope \
                                     for TASK 3B"
                .to_string(),
            candidate_rule: "no candidates".to_string(),
            known_exclusions: vec![
                "function imports".to_string(),
                "constant imports".to_string(),
            ],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_candidates_never_collapses_ambiguity() {
        let a = LinkTarget::file("a.rs");
        let b = LinkTarget::file("b.rs");
        assert!(matches!(
            LinkOutcome::from_candidates(vec![], "none"),
            LinkOutcome::Unresolved { .. }
        ));
        assert!(matches!(
            LinkOutcome::from_candidates(vec![a.clone()], "none"),
            LinkOutcome::Exact { .. }
        ));
        let ambiguous = LinkOutcome::from_candidates(vec![b.clone(), a.clone()], "none");
        assert_eq!(ambiguous.candidates().len(), 2);
        // Candidate order is deterministic regardless of input order.
        let other = LinkOutcome::from_candidates(vec![a, b], "none");
        assert_eq!(ambiguous, other);
    }

    #[test]
    fn rule_registry_documents_every_rule_exactly_once() {
        let registry = rule_registry();
        assert_eq!(registry.len(), rule::ALL.len());
        let mut ids: Vec<&str> = registry.iter().map(|r| r.rule_id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), rule::ALL.len());
        for rule_id in rule::ALL {
            assert!(registry.iter().any(|r| r.rule_id == *rule_id), "{rule_id}");
        }
    }

    #[test]
    fn link_ids_are_content_derived_and_occurrence_unique() {
        let first = FactLocator::import("a.go", 0);
        let second = FactLocator::import("a.go", 1);
        assert_ne!(
            link_id_for("r", &first, "x"),
            link_id_for("r", &second, "x")
        );
        assert_eq!(link_id_for("r", &first, "x"), link_id_for("r", &first, "x"));
        assert_ne!(
            link_id_for("r1", &first, "x"),
            link_id_for("r2", &first, "x")
        );
    }
}
