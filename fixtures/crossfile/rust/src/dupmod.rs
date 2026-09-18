// Two declarations with the same written name in one module. Rust would reject
// this semantically, but the syntax facts record both, so `crate::dupmod::dup`
// is genuinely ambiguous rather than first-wins.
pub fn dup() {}

pub fn dup() {}
