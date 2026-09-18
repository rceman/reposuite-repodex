//! A small exact query API over a derived link artifact.
//!
//! This is deliberately not a query language and not a search engine. Every
//! operation is an exact filter over derived relationships: by originating file,
//! by target, by outcome, by rule or by kind. There is no ranking, no fuzzy
//! match, no full-text index and no semantic search — TASK 3A's exact fact
//! lookup is the only other lookup surface, and it is equally exact.

use std::path::Path;

use super::artifact::{self, LinkError};
use super::model::{LinkManifest, LinkOutcome, LinkRecord, StructuralEntity};

/// A loaded link artifact, queryable by exact predicates.
#[derive(Debug, Clone)]
pub struct LinkIndex {
    manifest: LinkManifest,
    links: Vec<LinkRecord>,
    entities: Vec<StructuralEntity>,
}

impl LinkIndex {
    /// Load a link artifact from a directory.
    pub fn load(dir: &Path) -> Result<Self, LinkError> {
        let manifest = artifact::read_manifest(dir)?;
        let links = artifact::read_links(dir)?;
        let entities = artifact::read_entities(dir)?;
        Ok(Self {
            manifest,
            links,
            entities,
        })
    }

    pub fn manifest(&self) -> &LinkManifest {
        &self.manifest
    }

    pub fn links(&self) -> &[LinkRecord] {
        &self.links
    }

    pub fn entities(&self) -> &[StructuralEntity] {
        &self.entities
    }

    /// Relationships written in one file, in canonical order.
    pub fn from_source(&self, relative_path: &str) -> Vec<&LinkRecord> {
        self.links
            .iter()
            .filter(|link| link.source.relative_path == relative_path)
            .collect()
    }

    /// Relationships whose outcome names one file.
    pub fn targeting_file(&self, relative_path: &str) -> Vec<&LinkRecord> {
        self.links
            .iter()
            .filter(|link| {
                link.outcome
                    .all_targets()
                    .iter()
                    .any(|target| target.relative_path() == relative_path)
            })
            .collect()
    }

    /// Relationships whose outcome names one declaration.
    pub fn targeting_declaration(
        &self,
        relative_path: &str,
        declaration_id: u32,
    ) -> Vec<&LinkRecord> {
        self.links
            .iter()
            .filter(|link| {
                link.outcome
                    .all_targets()
                    .iter()
                    .any(|target| match target {
                        super::model::LinkTarget::Declaration {
                            relative_path: path,
                            declaration_id: id,
                            ..
                        } => path == relative_path && *id == declaration_id,
                        _ => false,
                    })
            })
            .collect()
    }

    fn with_outcome(&self, predicate: impl Fn(&LinkOutcome) -> bool) -> Vec<&LinkRecord> {
        self.links
            .iter()
            .filter(|link| predicate(&link.outcome))
            .collect()
    }

    /// Every `Exact` relationship.
    pub fn exact(&self) -> Vec<&LinkRecord> {
        self.with_outcome(|outcome| matches!(outcome, LinkOutcome::Exact { .. }))
    }

    /// Every `Ambiguous` relationship. Ambiguity is preserved, never collapsed.
    pub fn ambiguous(&self) -> Vec<&LinkRecord> {
        self.with_outcome(|outcome| matches!(outcome, LinkOutcome::Ambiguous { .. }))
    }

    /// Every `Unresolved` relationship.
    pub fn unresolved(&self) -> Vec<&LinkRecord> {
        self.with_outcome(|outcome| matches!(outcome, LinkOutcome::Unresolved { .. }))
    }

    /// Every `OutOfScope` relationship.
    pub fn out_of_scope(&self) -> Vec<&LinkRecord> {
        self.with_outcome(|outcome| matches!(outcome, LinkOutcome::OutOfScope { .. }))
    }

    /// Every relationship produced by one rule.
    pub fn by_rule(&self, rule_id: &str) -> Vec<&LinkRecord> {
        self.links
            .iter()
            .filter(|link| link.rule_id == rule_id)
            .collect()
    }

    /// Every relationship of one kind.
    pub fn by_kind(&self, kind: &str) -> Vec<&LinkRecord> {
        self.links.iter().filter(|link| link.kind == kind).collect()
    }

    /// One structural entity by id.
    pub fn entity(&self, entity_id: &str) -> Option<&StructuralEntity> {
        self.entities
            .iter()
            .find(|entity| entity.entity_id == entity_id)
    }

    /// Exact counts by rule id, in canonical order.
    pub fn counts_by_rule(&self) -> Vec<(String, u64)> {
        let mut counts: std::collections::BTreeMap<String, u64> = std::collections::BTreeMap::new();
        for link in &self.links {
            *counts.entry(link.rule_id.clone()).or_default() += 1;
        }
        counts.into_iter().collect()
    }
}
