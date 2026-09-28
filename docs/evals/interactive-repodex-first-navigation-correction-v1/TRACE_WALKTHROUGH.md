# Matched trace walkthrough
For a cd task (e.g. cd-res-doc): N0 fails the edit; P1 succeeds with injected
packet; R2 issues `./repo_query "..."`, receives adaptive RDX as TOOL_RESULT,
verifies source, edits -> SUCCESS. traces/ has the full observable sequence.
