//! Optional System One configuration (`~/reposuite/repodex/config.toml`).
//!
//! System One is OFF unless explicitly enabled. An absent file, an absent
//! `[system_one]` table, or `enabled=false` all mean the deterministic
//! pre-System-One behavior is used unchanged.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Canonical configuration file location.
pub fn config_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join("reposuite/repodex/config.toml")
}

/// Authentication for one named model endpoint.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Auth {
    #[default]
    None,
    Bearer { token: String },
    Header { header: String, token: String },
}

/// One named model endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelConfig {
    pub protocol: String,
    pub url: String,
    pub model: String,
    pub timeout_ms: u64,
    #[serde(default)]
    pub auth: Auth,
}

/// The `[system_one]` table.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SystemOneConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub roles: Roles,
    #[serde(default)]
    pub models: BTreeMap<String, ModelConfig>,
}

/// Role -> named model routing. A missing role means that role is disabled.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Roles {
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub rerank: Option<String>,
}

/// The whole config file. `[system_one]` may be absent entirely.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RepoDexConfig {
    #[serde(default)]
    pub system_one: Option<SystemOneConfig>,
}

/// Load the config file. Missing file -> default (System One disabled).
pub fn load(path: &std::path::Path) -> Result<RepoDexConfig, String> {
    if !path.exists() {
        return Ok(RepoDexConfig::default());
    }
    let text = std::fs::read_to_string(path).map_err(|e| format!("read {path:?}: {e}"))?;
    toml::from_str(&text).map_err(|e| format!("parse {path:?}: {e}"))
}

/// Save the config file (creates parent dirs).
pub fn save(path: &std::path::Path, cfg: &RepoDexConfig) -> Result<(), String> {
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p).map_err(|e| format!("mkdir: {e}"))?;
    }
    let text = toml::to_string_pretty(cfg).map_err(|e| format!("serialize: {e}"))?;
    std::fs::write(path, text).map_err(|e| format!("write {path:?}: {e}"))
}

/// Load the config file as a raw TOML document (for `config get/set` — does
/// not require the document to already be schema-valid).
pub fn load_value(path: &std::path::Path) -> Result<toml::Value, String> {
    if !path.exists() {
        return Ok(toml::Value::Table(Default::default()));
    }
    let text = std::fs::read_to_string(path).map_err(|e| format!("read {path:?}: {e}"))?;
    text.parse::<toml::Value>()
        .map_err(|e| format!("parse {path:?}: {e}"))
}

/// Save a raw TOML document.
pub fn save_value(path: &std::path::Path, doc: &toml::Value) -> Result<(), String> {
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p).map_err(|e| format!("mkdir: {e}"))?;
    }
    let text = toml::to_string_pretty(doc).map_err(|e| format!("serialize: {e}"))?;
    std::fs::write(path, text).map_err(|e| format!("write {path:?}: {e}"))
}

/// Get a `system-one.*` value from a raw document as a TOML scalar string.
pub fn get_key(doc: &toml::Value, key: &str) -> Option<String> {
    let mut cur = doc;
    for part in key.replace('-', "_").split('.') {
        cur = cur.get(part)?;
    }
    match cur {
        toml::Value::String(s) => Some(s.clone()),
        toml::Value::Boolean(b) => Some(b.to_string()),
        toml::Value::Integer(i) => Some(i.to_string()),
        toml::Value::Float(f) => Some(f.to_string()),
        _ => Some(cur.to_string()),
    }
}

/// Set a `system-one.*` key on a raw document (creating tables as needed).
/// The document need not yet be schema-valid — partial edits are allowed and
/// validation happens at `system-one status` / query time.
pub fn set_key(doc: &mut toml::Value, key: &str, value: &str) -> Result<(), String> {
    let parts: Vec<String> = key.split('.').map(|p| p.replace('-', "_")).collect();
    if parts.is_empty() {
        return Err("empty key".to_string());
    }
    let mut node = doc;
    for part in &parts[..parts.len() - 1] {
        if node.get(part).is_none() {
            node.as_table_mut()
                .ok_or("not a table")?
                .insert(part.clone(), toml::Value::Table(Default::default()));
        }
        node = node.get_mut(part).ok_or("nav")?;
        if !node.is_table() {
            return Err(format!("`{part}` is not a table"));
        }
    }
    let leaf = parts.last().unwrap();
    let parsed: toml::Value = toml::from_str::<toml::Table>(&format!("v={value}"))
        .ok()
        .and_then(|d| d.get("v").cloned())
        .unwrap_or_else(|| toml::Value::String(value.to_string()));
    node.as_table_mut()
        .ok_or("set leaf")?
        .insert(leaf.clone(), parsed);
    Ok(())
}

impl SystemOneConfig {
    /// Validate role bindings + model endpoints (§12). Errors never corrupt
    /// artifacts — they only gate whether System One may be used.
    pub fn validate(&self) -> Result<(), String> {
        for (role, name) in [("query", &self.roles.query), ("rerank", &self.roles.rerank)] {
            if let Some(n) = name {
                let m = self
                    .models
                    .get(n)
                    .ok_or_else(|| format!("role `{role}` references unknown model `{n}`"))?;
                m.validate(n)?;
            }
        }
        Ok(())
    }

    /// Resolve the model for a role, if enabled + assigned + valid.
    pub fn model_for(&self, role: &str) -> Option<(&String, &ModelConfig)> {
        if !self.enabled {
            return None;
        }
        let name = match role {
            "query" => self.roles.query.as_ref(),
            "rerank" => self.roles.rerank.as_ref(),
            _ => None,
        }?;
        self.models.get_key_value(name)
    }
}

impl ModelConfig {
    pub fn validate(&self, name: &str) -> Result<(), String> {
        if self.protocol != "system-one-v1" {
            return Err(format!(
                "model `{name}`: unsupported protocol `{}`",
                self.protocol
            ));
        }
        if self.url.is_empty() {
            return Err(format!("model `{name}`: missing url"));
        }
        if self.model.is_empty() {
            return Err(format!("model `{name}`: missing model name"));
        }
        if self.timeout_ms == 0 {
            return Err(format!("model `{name}`: timeout_ms must be > 0"));
        }
        match &self.auth {
            Auth::None => {}
            Auth::Bearer { token } => {
                if token.is_empty() {
                    return Err(format!("model `{name}`: bearer auth requires a token"));
                }
            }
            Auth::Header { header, token } => {
                if header.is_empty() || token.is_empty() {
                    return Err(format!("model `{name}`: header auth requires header+token"));
                }
            }
        }
        Ok(())
    }
}
