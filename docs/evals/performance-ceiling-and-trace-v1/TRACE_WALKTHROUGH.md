# Trace walkthrough — task qa-ct (crate name + edition)

All three arms are identical observable sessions; only the embedded packet differs.

## F0 Native
T+0  task: "What is the crate name and Rust edition declared in Cargo.toml?"
T+.. read /home/therceman/git/reposuite-repodex/Cargo.toml
T+.. ANSWER (correct).  in=27,673 tok, 1 tool (read).

## F1 JSON packet
T+0  task + 79KB JSON EvidenceProjection packet
T+.. read Cargo.toml   (verifies the packet's cargo facts)
T+.. ANSWER.           in=104,417 tok, 1 tool.

## F2 RDX packet
T+0  task + ~22KB RDX packet
T+.. read Cargo.toml
T+.. ANSWER.           in=52,540 tok, 1 tool.

## Delta
All arms performed the SAME single verification read — the packet did not
eliminate work on a trivial manifest lookup; it only added input tokens.
RDX halves the JSON packet's token cost (52.5k vs 104.4k vs 27.7k native).
On cross-cutting (cc) questions the packet DID reduce searches (see benchmark),
so packet value is workload-dependent — it buys less verification on multi-hop
tasks, costs tokens on single-hop ones.
