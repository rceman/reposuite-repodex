//! Recipe store (§37): incremental, versioned, project-scoped, JSON-backed,
//! rebuildable from canonical history. No SQLite.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::model::{RecipeDefinition, RECIPE_SCHEMA};
use crate::repository::digest;

#[derive(Debug)]
pub enum RecipeError {
    Io(std::io::Error),
    Json(serde_json::Error),
}
impl From<std::io::Error> for RecipeError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
impl From<serde_json::Error> for RecipeError {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}

/// Persistent bounded recipe store, partitioned by project scope.
#[derive(Debug, Default)]
pub struct RecipeStore {
    /// recipe_id -> definition.
    pub recipes: BTreeMap<String, RecipeDefinition>,
}

impl RecipeStore {
    fn dir(base: &Path, scope: &str) -> PathBuf {
        // Partition by scope digest so unrelated projects never share recipes.
        let d = digest::content_digest(scope.as_bytes());
        base.join("recipes").join(d.replace(':', "_"))
    }
    /// Load all recipes for `scope`.
    pub fn load(base: &Path, scope: &str) -> Result<Self, RecipeError> {
        let mut s = Self::default();
        let dir = Self::dir(base, scope);
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                if e.path().extension().map(|x| x == "json").unwrap_or(false) {
                    let r: RecipeDefinition =
                        serde_json::from_str(&std::fs::read_to_string(e.path())?)?;
                    s.recipes.insert(r.recipe_id.clone(), r);
                }
            }
        }
        Ok(s)
    }
    /// Persist (insert or replace) a recipe — atomic write via tmp+rename.
    pub fn upsert(&mut self, base: &Path, r: RecipeDefinition) -> Result<(), RecipeError> {
        let dir = Self::dir(base, &r.project_scope);
        std::fs::create_dir_all(&dir)?;
        let tmp = dir.join(format!(".{}.tmp", r.recipe_id));
        let dst = dir.join(format!("{}.json", r.recipe_id));
        std::fs::write(&tmp, serde_json::to_vec_pretty(&r)?)?;
        std::fs::rename(&tmp, &dst)?;
        self.recipes.insert(r.recipe_id.clone(), r);
        Ok(())
    }
    /// Deterministic recipe id from scope+family+anchor terms.
    pub fn recipe_id(scope: &str, family: &str, terms: &[String]) -> String {
        let key = format!("{}|{}|{}", scope, family, terms.join(","));
        format!(
            "{}:{}",
            RECIPE_SCHEMA,
            digest::content_digest(key.as_bytes())
        )
    }
}
