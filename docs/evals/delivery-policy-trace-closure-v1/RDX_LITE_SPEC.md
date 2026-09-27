# RDX-LITE

Tiny bootstrap: the RDX header + top 1-2 anchors (strongest seed path+locator+FACT)
under a hard <=2KiB budget, plus an expansion handle (`repo_query`). Omits the
full related-edge set and lower-ranked seeds. Preserves FACT/CANDIDATE and any
ambiguity/gap marker — never converts CANDIDATE to FACT. Full RDX on demand.
