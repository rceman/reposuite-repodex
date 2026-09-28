# Continuous project-learning foundation (V1) — deterministic only, Jev off.
Two planes: CURRENT_REPOSITORY_TRUTH vs LEARNED_PROJECT_NAVIGATION_KNOWLEDGE.
- inventory()      deterministic project inventory (langs/manifests/modules/
                   entrypoints/tests) — no LLM rediscovery.
- coverage()       view-bound ledger: KNOWN/PARTIAL/UNRESOLVED/UNSUPPORTED/
                   NOT_APPLICABLE (each NA carries a deterministic reason).
- generate_questions()  only from coverage gaps — auditable QuestionCandidate
                   with origin + priority factors; never free-form curiosity.
- question_priority()   deterministic si*4+ur*3+dm*2-cost-churn.
- MemoryArtifact        stores family+anchors+route+dependencies — HOW to
                   investigate, not WHAT is true.
- rebind_memory()       CurrentValid/Partial/Stale/Ambiguous/Missing/Unsupported
                   — anchors must resolve uniquely; changed deps -> Stale.
- promote()/demote()    conservative lifecycle; no auto-high-priority memory.
