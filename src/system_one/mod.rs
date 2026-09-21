//! Optional System One integration.
//!
//! System One is a **bounded decision accelerator**, never repository truth.
//! RepoDex constructs the option set; System One `choice`/`score`/`noul`
//! answers may only influence ordering/advice; RepoDex validates every answer
//! and falls back to the deterministic baseline on any failure.
//!
//! Disabled (default) => byte-identical deterministic behavior.

pub mod client;
pub mod config;
pub mod protocol;
pub mod service;

pub use client::{HttpSystemOneModel, SystemOneError, SystemOneModel};
pub use config::{Auth, ModelConfig, RepoDexConfig, Roles, SystemOneConfig};
pub use protocol::{
    Answer, ChoiceA, NoulA, Question, ScoreA, SystemOneRequest, SystemOneResponse, PROTOCOL_V1,
};
pub use service::{Provenance, RoleStatus, SystemOne};
