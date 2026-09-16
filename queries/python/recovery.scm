; Recovery artifacts produced by the Tree-sitter runtime itself.
;
; `ERROR` marks input the parser could not attach to any grammar rule.
; `MISSING` marks a zero-width token the parser inserted to continue.
;
; These are runtime facts about the parse, not claims about the source.

(ERROR) @error
(MISSING) @missing
