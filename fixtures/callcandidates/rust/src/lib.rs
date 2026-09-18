use crate::other::imported_fn;
pub use crate::other::reexported_fn;

mod a;
mod b;
mod lonely;
mod other;

fn helper() {}
fn run() {
    helper();
}
fn missing() {
    absent();
}
fn run_imported() {
    imported_fn();
}
fn run_reexport() {
    reexported_fn();
}
fn outer() {
    fn helper() {}
    helper();
}
