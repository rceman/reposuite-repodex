//! RDX1 — the compact, deterministic, agent-facing result protocol (Phase C).
//!
//! A line-oriented projection of a deterministic `QueryResult`. The default
//! `query` output; JSON/human remain opt-in. See `docs/RDX1.md`.
//!
//! ```text
//! #RDX1 v1
//! Q <raw query>
//! F <lid> <kind> <key> <label>          # define a node as local id <lid>
//! R <rel> <src> <tgt> <ev> [cs=<n>]     # relationship, ev = f|c
//! S <k>=<v> ...                          # summary/completeness
//! ```
//!
//! Local ids are small integers assigned in canonical (key) order — stable and
//! independent of the persistent `gn-`/`ge-` ids, which cost more tokens.
//! `ev=f` is a FACT edge, `ev=c` a CANDIDATE edge; `cs` groups a
//! MultipleCandidates set. Output is byte-identical for identical input.

use std::collections::BTreeMap;

use crate::graph::{EvidenceClass, GraphNode};
use crate::query::QueryResult;

/// The RDX1 protocol version.
pub const RDX1_VERSION: u32 = 1;

/// A parsed RDX1 document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rdx1Doc {
    pub query: Option<String>,
    /// lid -> (kind, key, label)
    pub facts: Vec<Rdx1Fact>,
    pub relations: Vec<Rdx1Relation>,
    pub summary: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rdx1Fact {
    pub lid: u32,
    pub kind: String,
    pub key: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rdx1Relation {
    pub rel: String,
    pub src: u32,
    pub tgt: u32,
    /// `f` fact / `c` candidate.
    pub evidence: char,
    pub candidate_set: Option<String>,
}

/// Render a `QueryResult` as RDX1 text.
pub fn render(result: &QueryResult) -> String {
    // Assign local ids in canonical key order for determinism.
    let mut lid_of: BTreeMap<String, u32> = BTreeMap::new();
    let mut order: Vec<&GraphNode> = Vec::new();
    for s in &result.seeds {
        order.push(&s.node);
    }
    for r in &result.related {
        order.push(&r.node);
    }
    order.sort_by(|a, b| a.key.cmp(&b.key));
    order.dedup_by(|a, b| a.node_id == b.node_id);
    let mut out = String::from("#RDX1 v1\n");
    out.push_str(&format!("Q {}\n", escape(&result.plan.raw)));
    let mut next = 1u32;
    for n in &order {
        lid_of.insert(n.node_id.clone(), next);
        out.push_str(&format!(
            "F {} {} {} {}\n",
            next,
            n.kind.as_str(),
            escape(&n.key),
            escape(&n.label)
        ));
        next += 1;
    }
    // Relationships: emit each `related` edge as seed -> neighbor.
    let key_to_id: BTreeMap<&str, u32> = order
        .iter()
        .filter_map(|n| lid_of.get(&n.node_id).map(|&l| (n.key.as_str(), l)))
        .collect();
    for r in &result.related {
        let Some(&rel_lid) = lid_of.get(&r.node.node_id) else {
            continue;
        };
        // `via` is the seed's canonical key -> its local id.
        let Some(&seed_lid) = key_to_id.get(r.via.as_str()) else {
            continue;
        };
        // Preserve true edge direction: an `incoming` relation is neighbor->
        // seed, an `outgoing` relation is seed->neighbor.
        let (src, tgt) = if r.direction == "incoming" {
            (rel_lid, seed_lid)
        } else {
            (seed_lid, rel_lid)
        };
        let ev = match r.evidence {
            EvidenceClass::Fact => 'f',
            EvidenceClass::Candidate => 'c',
        };
        out.push_str(&format!("R {} {} {} {}", r.kind, src, tgt, ev));
        if let Some(cs) = r.candidate_set.as_deref() {
            out.push_str(&format!(" cs={cs}"));
        }
        out.push('\n');
    }
    // Summary.
    let cand = result
        .related
        .iter()
        .filter(|r| r.evidence == EvidenceClass::Candidate)
        .count();
    out.push_str(&format!(
        "S shown={} total={} complete={} seeds={} related={} candidates={}\n",
        result.shown,
        result.total,
        u8::from(result.complete),
        result.seeds.len(),
        result.related.len(),
        cand
    ));
    if let Some(reason) = &result.truncated_reason {
        out.push_str(&format!("S truncated=1 reason={}\n", reason));
    }
    out
}

/// Parse RDX1 text back into a document (round-trip / validation).
pub fn parse(text: &str) -> Result<Rdx1Doc, String> {
    let mut doc = Rdx1Doc {
        query: None,
        facts: Vec::new(),
        relations: Vec::new(),
        summary: BTreeMap::new(),
    };
    let mut saw_header = false;
    for line in text.lines() {
        let line = line.trim_end();
        if line.is_empty() {
            continue;
        }
        if line == "#RDX1 v1" || line.starts_with("#RDX1") {
            saw_header = true;
            continue;
        }
        let mut it = line.split_whitespace();
        match it.next() {
            Some("Q") => doc.query = Some(unescape(&line[2..])),
            Some("F") => {
                let lid: u32 = it.next().and_then(|s| s.parse().ok()).ok_or("bad F lid")?;
                let kind = it.next().ok_or("bad F kind")?.to_string();
                let key = it.next().map(unescape).ok_or("bad F key")?;
                let label = unescape(it.next().unwrap_or(""));
                doc.facts.push(Rdx1Fact {
                    lid,
                    kind,
                    key,
                    label,
                });
            }
            Some("R") => {
                let rel = it.next().ok_or("bad R rel")?.to_string();
                let src: u32 = it.next().and_then(|s| s.parse().ok()).ok_or("bad R src")?;
                let tgt: u32 = it.next().and_then(|s| s.parse().ok()).ok_or("bad R tgt")?;
                let evidence = it.next().and_then(|s| s.chars().next()).ok_or("bad R ev")?;
                let candidate_set = it.find_map(|p| p.strip_prefix("cs=").map(|s| s.to_string()));
                doc.relations.push(Rdx1Relation {
                    rel,
                    src,
                    tgt,
                    evidence,
                    candidate_set,
                });
            }
            Some("S") => {
                for tok in it {
                    if let Some((k, v)) = tok.split_once('=') {
                        doc.summary.insert(k.to_string(), v.to_string());
                    }
                }
            }
            _ => {}
        }
    }
    if !saw_header {
        return Err("missing #RDX1 header".to_string());
    }
    Ok(doc)
}

fn escape(s: &str) -> String {
    if s.contains(char::is_whitespace) {
        // quote + backslash-escape for whitespace-bearing values
        let mut o = String::from("\"");
        for c in s.chars() {
            match c {
                '"' => o.push_str("\\\""),
                '\\' => o.push_str("\\\\"),
                _ => o.push(c),
            }
        }
        o.push('"');
        o
    } else {
        s.to_string()
    }
}

fn unescape(s: &str) -> String {
    let s = s.trim();
    if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
        s[1..s.len() - 1]
            .replace("\\\"", "\"")
            .replace("\\\\", "\\")
    } else {
        s.to_string()
    }
}

/// Approximate token estimate for a response (documented estimator: ~4 chars/token,
/// a common heuristic — not a model-specific tokenizer).
pub fn estimate_tokens(text: &str) -> usize {
    text.len().div_ceil(4)
}
