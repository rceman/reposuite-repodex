//! `system-one-v1` — the bounded decision protocol.
//!
//! A request is `{model, state, questions}`; questions are typed `choice`,
//! `score` or `noul`. System One returns typed *decisions*, never repository
//! facts or arbitrary plans. RepoDex constructs the option set and validates
//! every answer before it can influence ordering/advice.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Supported protocol identifier.
pub const PROTOCOL_V1: &str = "system-one-v1";

/// A `choice` question: pick one of the offered options.
#[derive(Debug, Clone, Serialize)]
pub struct ChoiceQ {
    #[serde(rename = "type")]
    pub kind: &'static str, // "choice"
    pub instructions: String,
    /// option-id -> human description (bounded set built by RepoDex).
    pub criteria: BTreeMap<String, String>,
}

/// A `score` question: rate one thing on an ordered scale.
#[derive(Debug, Clone, Serialize)]
pub struct ScoreQ {
    #[serde(rename = "type")]
    pub kind: &'static str, // "score"
    pub instructions: String,
    /// ordered level descriptions (RepoDex owns the interpretation).
    pub criteria: Vec<String>,
}

/// A `noul` question: a yes/no probability, the answer is the probability.
#[derive(Debug, Clone, Serialize)]
pub struct NoulQ {
    #[serde(rename = "type")]
    pub kind: &'static str, // "noul"
    pub instructions: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub criteria: Option<Vec<String>>,
}

/// One question (keyed by an application id).
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum Question {
    Choice(ChoiceQ),
    Score(ScoreQ),
    Noul(NoulQ),
}

impl Question {
    pub fn choice(instructions: impl Into<String>, criteria: BTreeMap<String, String>) -> Self {
        Question::Choice(ChoiceQ {
            kind: "choice",
            instructions: instructions.into(),
            criteria,
        })
    }
    pub fn score(instructions: impl Into<String>, criteria: Vec<String>) -> Self {
        Question::Score(ScoreQ {
            kind: "score",
            instructions: instructions.into(),
            criteria,
        })
    }
    pub fn noul(instructions: impl Into<String>) -> Self {
        Question::Noul(NoulQ {
            kind: "noul",
            instructions: instructions.into(),
            criteria: None,
        })
    }
}

/// A `system-one-v1` request.
#[derive(Debug, Clone, Serialize)]
pub struct SystemOneRequest {
    pub protocol: String,
    pub model: String,
    pub state: Value,
    /// question-id -> typed question.
    pub questions: BTreeMap<String, Question>,
}

impl SystemOneRequest {
    pub fn new(model: impl Into<String>, state: Value) -> Self {
        Self {
            protocol: PROTOCOL_V1.to_string(),
            model: model.into(),
            state,
            questions: BTreeMap::new(),
        }
    }
}

/// A typed `choice` answer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChoiceA {
    pub choice: String,
    #[serde(default)]
    pub probabilities: Value,
    #[serde(default)]
    pub confidence: f64,
}

/// A typed `score` answer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoreA {
    pub score: f64,
    #[serde(default)]
    pub legend: Value,
    #[serde(default)]
    pub probabilities: Value,
    #[serde(default)]
    pub confidence: f64,
}

/// A typed `noul` answer — the probability itself is the result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoulA {
    pub noul: f64,
}

/// One typed answer. `#[serde(untagged)]` lets providers add harmless fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Answer {
    Choice(ChoiceA),
    Score(ScoreA),
    Noul(NoulA),
}

/// A `system-one-v1` response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemOneResponse {
    /// question-id -> typed answer.
    pub answers: BTreeMap<String, Answer>,
    /// Harmless provider extras (usage, latency, …) — tolerated, ignored.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// Validate a response's answers are well-formed (finite numbers, sane
/// ranges) so a malformed response cannot influence query behavior (§19).
pub fn validate_response(resp: &SystemOneResponse) -> Result<(), String> {
    for (id, a) in &resp.answers {
        let check = |v: f64, what: &str| -> Result<(), String> {
            if !v.is_finite() {
                return Err(format!("answer `{id}`: non-finite {what}"));
            }
            Ok(())
        };
        match a {
            Answer::Choice(c) => {
                if c.choice.is_empty() {
                    return Err(format!("answer `{id}`: empty choice"));
                }
                check(c.confidence, "confidence")?;
                if !(0.0..=1.0).contains(&c.confidence) {
                    return Err(format!("answer `{id}`: confidence out of range"));
                }
            }
            Answer::Score(s) => {
                check(s.score, "score")?;
                check(s.confidence, "confidence")?;
            }
            Answer::Noul(n) => {
                check(n.noul, "noul")?;
                if !(0.0..=1.0).contains(&n.noul) {
                    return Err(format!("answer `{id}`: noul out of [0,1]"));
                }
            }
        }
    }
    Ok(())
}
