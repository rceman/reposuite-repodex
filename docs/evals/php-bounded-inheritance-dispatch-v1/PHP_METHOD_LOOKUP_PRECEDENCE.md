# Nearest-level method lookup

For a bounded receiver class C and method `m`:

1. level 0: directly declared `m` in C (case-insensitive). Any hit stops the
   walk and keeps the receiver's own rule id.
2. level d>0: the direct parent candidates of every frontier class
   (`php.class.extends` targets; Ambiguous parents are all followed; visited
   set + depth bound 64 makes cycles terminate).
3. the FIRST ancestor level containing `m` candidates wins and traversal
   stops — an override suppresses deeper ancestor candidates (verified: D
   overrides B/A `over`; `parent::over` from D starts at C and lands on B).

Visibility: PHP is class-scoped — the lexical call scope may reach a private
`m` only when the searched class IS that scope's class at level 0.
Ancestor-private methods are never reachable (verified: `$this->hidden` and
`A $a -> $a->hidden` produce no candidate; previously `A::hidden` was
erroneously surfaced — a false candidate now eliminated).

Multiple bounded receiver classes each run the walk and union; the reported
`ancestor_depth` is the maximum winning depth.
