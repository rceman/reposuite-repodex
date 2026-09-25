# Manifest / Module / Package Intelligence V1

go.mod and Cargo.toml were validity-only metadata (consulted for RepositoryView
content identity, never queryable). This turns them into first-class indexed
artifacts: each is parsed into typed facts emitted as `Declaration`s on a
synthetic `FileAnalysis` (language=`manifest`), so the artifact flows through
the same content-addressed snapshot -> Investigation Graph -> term index path
as source files. Ownership edges (`owned_by_manifest`) connect a source file to
its nearest enclosing manifest, respecting nested module boundaries.
