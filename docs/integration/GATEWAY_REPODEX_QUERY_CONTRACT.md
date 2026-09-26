# Gateway <-> RepoDex Query Authority Contract (v1)

## Ownership

GPT Tunnel Gateway owns Task/worktree/delegation authority. RepoDex is
Task-neutral: it answers repository questions about an explicit RepositoryView
and never decides which Task or worktree the caller may use.

    ChatGPT/Agent -> Gateway (resolves gateway, project, Task, worktree root)
                  -> RepoDex /v1/query { root: <exact path>, ... }

RepoDex does NOT: parse Task, infer delegation, pick a worktree, or fall back to
daemon cwd / registered project when an explicit `root` is expected.

## Required request

`POST /v1/query` (bearer token in `Authorization`):

    {
      "root":  "<exact worktree/repo path>",     // REQUIRED — the authority
      "query": "<natural-language question>",    // REQUIRED
      // optional, bounded — see PRODUCT_PROFILE_V1
    }

`root` is content-keyed (current filesystem bytes + metadata), not a branch
name. A branch is metadata; the RepositoryView is authoritative.

## Recommended production profile

External integrations should send only `root` + `query` and accept the defaults
(`policy_profile = production-v1`): manifest intelligence on; memory, recipes,
utility, source-witness, and vocabulary bridges all off unless explicitly
required. RepoDex should not require callers to know the experimental switches.

## Error model (stable)

`error.code`: `UNAUTHORIZED`, `INVALID_REQUEST`, `INVALID_ROOT`,
`VIEW_INVALID`, `QUERY_UNRESOLVED`, `QUERY_AMBIGUOUS`, `BUDGET_EXHAUSTED`,
`SERVICE_UNAVAILABLE`, `INTERNAL`. "No evidence" is a 200 with empty seeds —
not a server error.

## What RepoDex does not own

Task semantics, delegation, user authorization, cross-project routing.
