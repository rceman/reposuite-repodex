//! RepoDex Service — persistent local loopback service + tiered runtime (V1).
//!
//! Lifecycle (`serve`/`start`/`stop`/`restart`/`status`), loopback HTTP API,
//! bearer-token auth, dynamic-port runtime discovery, HOT/WARM/COLD tiers,
//! batched durable AgentEvent ingest, and asynchronous derived processing.
//! No SQLite; no OS daemon installer; harness-neutral.

pub mod api;
pub mod config;
pub mod derived;
pub mod http;
pub mod lifecycle;
pub mod runtime;
pub mod segments;
pub mod state;
pub mod token;
pub mod util;

pub use config::ServiceConfig;
pub use lifecycle::{restart, serve, start, status, stop};
pub use runtime::RuntimeDescriptor;
