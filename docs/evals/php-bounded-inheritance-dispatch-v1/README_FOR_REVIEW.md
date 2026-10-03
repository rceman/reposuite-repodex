# PHP Bounded Inheritance Dispatch V1 — review map

- `PHP_HIERARCHY_CONTRACT.md` — FACT/CANDIDATE/UNSUPPORTED table.
- `PHP_METHOD_LOOKUP_PRECEDENCE.md` — nearest-level + visibility rules.
- `PHP_UNSUPPORTED_DISPATCH_BOUNDARY.md` — explicit exclusions.
- `MECHANICAL_GATE.json` / `MECHANICAL_PROBES.json` /
  `MECHANICAL_PACKET_DIFFS.json` — H0-vs-H1 disposition diffs (19 gains,
  1 intentional false-candidate removal, reason-string renames).
- `PERFORMANCE.json` — repodex-bench php H0 vs H1.
- `TRANSPORT_PARITY.json` — direct/service/auto byte-parity.
- `PILOT_*` — the 36-session headroom pilot (12 tasks × N0/H0/H1 × 1 rep).
- `REPORT.md` — verdicts + all required answers.
