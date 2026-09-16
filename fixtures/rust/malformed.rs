// Fixture: Rust recovery.
//
// * `valid_before` and `valid_after` surround a damaged struct body and must
//   survive recovery.
// * `fn incomplete(` is unterminated, so recovery consumes the following
//   `fn valid_last() {}` as a function-type parameter. `valid_last` is therefore
//   not a declaration, and RepoDex does not fabricate one.
// * `impl Incomplete` has no body but still yields an impl scope and an
//   unresolved `impl_target` reference.

fn valid_before() {}

struct Broken {
    field: ,
}

fn valid_after() {}

fn incomplete(

fn valid_last() {}

impl Incomplete
