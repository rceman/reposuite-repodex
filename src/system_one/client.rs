//! Provider-neutral System One transport.
//!
//! `SystemOneModel` is the abstraction; `HttpSystemOneModel` is a bounded
//! synchronous HTTP(S) implementation using `ureq` (no async runtime). The
//! configured full URL is authoritative — nothing is Jev- or provider-specific.

use std::io::Read;

use super::config::{Auth, ModelConfig};
use super::protocol::{validate_response, SystemOneRequest, SystemOneResponse};

/// A typed decision request/response abstraction (§13).
pub trait SystemOneModel {
    fn decide(&self, request: &SystemOneRequest) -> Result<SystemOneResponse, SystemOneError>;
    /// The config instance name (`[models.<name>]`).
    fn name(&self) -> &str;
    /// The provider model id sent on the wire (`model` field).
    fn model_id(&self) -> &str;
}

/// System One failure taxonomy feeding deterministic fallback (§21).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SystemOneError {
    Timeout,
    Connection(String),
    Auth(String),
    RateLimited,
    Server(u16),
    InvalidJson(String),
    InvalidResponse(String),
    UnsupportedProtocol(String),
    Config(String),
}

impl std::fmt::Display for SystemOneError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Timeout => write!(f, "timeout"),
            Self::Connection(e) => write!(f, "connection: {e}"),
            Self::Auth(e) => write!(f, "auth: {e}"),
            Self::RateLimited => write!(f, "rate limited"),
            Self::Server(c) => write!(f, "server error {c}"),
            Self::InvalidJson(e) => write!(f, "invalid json: {e}"),
            Self::InvalidResponse(e) => write!(f, "invalid response: {e}"),
            Self::UnsupportedProtocol(p) => write!(f, "unsupported protocol `{p}`"),
            Self::Config(e) => write!(f, "config: {e}"),
        }
    }
}
impl std::error::Error for SystemOneError {}

/// Cap a provider response so a hostile/huge body can't exhaust memory.
const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;

/// A synchronous HTTP(S) `system-one-v1` client for one named endpoint.
pub struct HttpSystemOneModel {
    name: String,
    cfg: ModelConfig,
    agent: ureq::Agent,
}

impl HttpSystemOneModel {
    pub fn new(name: impl Into<String>, cfg: &ModelConfig) -> Result<Self, SystemOneError> {
        let name = name.into();
        if cfg.protocol != super::protocol::PROTOCOL_V1 {
            return Err(SystemOneError::UnsupportedProtocol(cfg.protocol.clone()));
        }
        cfg.validate(&name).map_err(SystemOneError::Config)?;
        let agent = ureq::AgentBuilder::new()
            .timeout(std::time::Duration::from_millis(cfg.timeout_ms))
            .timeout_connect(std::time::Duration::from_millis(cfg.timeout_ms))
            .max_idle_connections(0)
            .build();
        Ok(Self {
            name,
            cfg: cfg.clone(),
            agent,
        })
    }
}

impl SystemOneModel for HttpSystemOneModel {
    fn name(&self) -> &str {
        &self.name
    }

    fn model_id(&self) -> &str {
        &self.cfg.model
    }

    fn decide(&self, request: &SystemOneRequest) -> Result<SystemOneResponse, SystemOneError> {
        let body = serde_json::to_string(request)
            .map_err(|e| SystemOneError::InvalidJson(e.to_string()))?;
        let mut call = self
            .agent
            .post(&self.cfg.url)
            .set("Content-Type", "application/json");
        match &self.cfg.auth {
            Auth::None => {}
            Auth::Bearer { token } => {
                call = call.set("Authorization", &format!("Bearer {token}"));
            }
            Auth::Header { header, token } => {
                call = call.set(header, token);
            }
        }
        let resp = match call.send_string(&body) {
            Ok(r) => r,
            Err(ureq::Error::Status(401, _)) => {
                return Err(SystemOneError::Auth("401 unauthorized".to_string()))
            }
            Err(ureq::Error::Status(403, _)) => {
                return Err(SystemOneError::Auth("403 forbidden".to_string()))
            }
            Err(ureq::Error::Status(429, _)) => return Err(SystemOneError::RateLimited),
            Err(ureq::Error::Status(c, _)) if c >= 500 => return Err(SystemOneError::Server(c)),
            Err(ureq::Error::Status(c, _)) => return Err(SystemOneError::Server(c)),
            Err(ureq::Error::Transport(t)) => {
                let msg = t.to_string();
                if msg.contains("timed out") || msg.contains("timeout") {
                    return Err(SystemOneError::Timeout);
                }
                return Err(SystemOneError::Connection(msg));
            }
        };
        // Bounded read.
        let mut buf = Vec::new();
        resp.into_reader()
            .take(MAX_RESPONSE_BYTES as u64 + 1)
            .read_to_end(&mut buf)
            .map_err(|e| SystemOneError::Connection(e.to_string()))?;
        if buf.len() > MAX_RESPONSE_BYTES {
            return Err(SystemOneError::InvalidResponse(
                "response too large".to_string(),
            ));
        }
        let parsed: SystemOneResponse =
            serde_json::from_slice(&buf).map_err(|e| SystemOneError::InvalidJson(e.to_string()))?;
        validate_response(&parsed).map_err(SystemOneError::InvalidResponse)?;
        Ok(parsed)
    }
}
