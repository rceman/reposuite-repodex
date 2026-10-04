# Errata (CORRECTION-V1)

- `artifact_presentations`=415; presentations with >=1 gap signature = **330**,
  not 415 (85 had no gap). Family buckets overlap; their sums exceed 330 by
  design.
- The V1 classifier treated uncorrelated `shell`/`other_repository_tool` calls
  as task action — corrected: they are now `unclassified_repository_activity`
  unless `repository_operation` proves otherwise (see
  `docs/evals/production-gap-telemetry-correction-v1/`).
- The V1 observation missed `no_candidate:<reason>`/`out_of_scope:<reason>`
  dispositions — corrected via structured `disposition_kind`/`disposition_reason`
  (graph schema 3). Candidate reason codes now reach `gap_signatures`.
- REPLAY_RESULTS.json regenerated under the corrected implementation.
