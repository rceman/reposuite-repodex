# Deterministic Query Engine

`reposuite-repodex query` is a **model-free** lexical + graph retrieval layer over the
investigation graph. It is not an LLM and never will be — it answers
reproducible lexical + structural questions and reports completeness honestly.
A future System One model may produce the same `QueryPlan`, never repository
facts.

```bash
reposuite-repodex query --graph <graph> "auth token"                # ranked find (RDX1)
reposuite-repodex query --graph <graph> --exhaustive "auth"         # every match
reposuite-repodex query --graph <graph> --intent callers --target ValidateToken x
reposuite-repodex query --graph <graph> --explain "auth token"      # inspectable plan
reposuite-repodex query --graph <graph> --tokens 400 "auth"         # output budget
reposuite-repodex query --graph <graph> --json "..."  / --human     # opt-in formats
```

## Pipeline

```text
raw text -> QueryPlan -> lexical seed lookup -> bounded expansion
        -> deterministic ranking -> QueryResult
```

- **Seed lookup**: normalized identifier terms (see `identifier_terms`) are
  matched against a term index built over declaration names, entity/package
  names and file-path components. No source reparse, no comments/docstrings.
- **Expansion**: per intent — `callers` (incoming `call_candidate`), `callees`
  (contained call sites -> candidate targets), `find`/`related` (one-hop
  neighborhood). Bounded by `max_results`/`max_depth`.
- **Ranking**: integer score = `1000·exact + 400·all_terms + 100·term_hits +
  30·path_hits + kind_prior`. Score is ordering metadata only — it is never a
  certainty signal (a top-ranked `call_candidate` is still a CANDIDATE).

## Modes

`ranked` (default) orders by score and caps at `max_results`. `exhaustive`
returns every source-grounded match; if a `--max-results`/token budget truncates
the presentation it reports `complete=false` + `total` — never silent omission.

## Workflows (all model-free)

```text
find symbol          -> query "Name"            / graph find
candidate callers    -> query --intent callers --target Name x
candidate callees    -> query --intent callees --target fn x
related entities     -> query --intent related "term" / graph neighborhood
candidate paths      -> graph paths A B --depth N
exhaustive enum      -> query --exhaustive "term"
```

Every result keeps its evidence class (`f`/`c` in RDX1) and candidate-set id so
uncertainty is never hidden.
