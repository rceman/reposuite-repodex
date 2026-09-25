//! Current-source witness delivery: exact bounded source bytes from the
//! validated view, never navigation metadata alone.

pub mod materialize;
pub mod model;

pub use materialize::{
    materialize, WitnessPacket, MAX_TOTAL_WITNESS_BYTES, MAX_WITNESSES, MAX_WITNESS_BYTES,
};
pub use model::{CurrentSourceWitness, WitnessRole, WITNESS_SCHEMA};

/// Source-witness query policy (§16): off | bounded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceWitnessPolicy {
    Off,
    Bounded,
}
impl SourceWitnessPolicy {
    pub fn parse(s: &str) -> Self {
        match s {
            "bounded" => Self::Bounded,
            _ => Self::Off,
        }
    }
}
