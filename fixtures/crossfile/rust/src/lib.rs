// Curated Rust cross-file fixture (TASK 3B).
//
// `mod api;` is ambiguous because both `src/api.rs` and `src/api/mod.rs` exist.
// `mod absent;` has no candidate file at all.
// `mod outer { mod inner; }` resolves through an inline module.

mod api;
mod util;
mod absent;
mod deep;
mod dupmod;

mod outer {
    mod inner;
}

use crate::util::helper;
use crate::util::missing_item;
use crate::util::{helper, Other};
use crate::util::*;
use crate::deep::helper;
use crate::dupmod::dup;
use crate::absent::thing;
use crate::outer::inner::inner_fn;
use std::collections::HashMap;
use self::util::helper as aliased;
