# Pilot trace walkthrough

Representative observable behavior (all 36 traces committed under `traces/`):

- `inh-parent-handle.P2.r1` — H1 agent calls `repo_query` twice (0 native
  searches) and answers `parent::handle -> BaseService::handle` from the
  candidate packet, 12.2s wall.
- `inh-orders-save.P2.r1` — typed-receiver + ancestor walk surfaces
  `OrderRepo -> BaseRepo::save`; zero native discovery.
- `inh-audited-trait.*.r1` — trait control: all arms answer `Auditable`
  via source reads; no false trait-dispatch candidate exists (H1 keeps the
  structural `class_uses_trait` edge only).
- `inh-static-boot.*.r1` — `static::` stays `late_static_binding`
  out-of-scope in H1; agents resolve `Jobs::boot` by reading source.
