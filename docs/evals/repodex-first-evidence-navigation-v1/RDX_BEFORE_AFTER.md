# RDX before (full, F1) vs adaptive (A2)

Query `Who calls ResolveKey?` — F1 emits the whole bounded neighborhood
(~13.9kB avg). A2 emits the caller edge + locators only:

## adaptive (A2)
I callers
F 1 declaration decl:internal/engine/core.go#4 ResolveKey
F 2 node file:internal/engine/core.go internal/engine/core.go
D 1 rank=1 path=internal/engine/core.go decl=13:186-315 body=13:230-315
R contains 2 1 f rule=repodex.containment
S ... intent=callers

Removed: unrelated seeds, generic containment/membership neighborhood edges the
agent never used. Retained: the answer-bearing edge + exact source locator.
