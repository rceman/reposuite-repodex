#![allow(dead_code)]

//! Fixture: Rust boundary probes.

use core::fmt::Debug;

macro_rules! define_thing {
    () => {};
}

pub struct Wrapper<'a, T: Debug>
where
    T: Clone,
{
    inner: &'a T,
}

impl<'a, T: Debug + Clone> Wrapper<'a, T> {
    pub async fn process<U>(&self, value: U) -> U
    where
        U: Send,
    {
        value
    }
}

pub unsafe fn dangerous() {}

pub const fn constant_fn() -> u32 {
    0
}

pub fn raw_identifiers() {
    let r#type = 1;
    let r#match = 2;
    define_thing!();
    println!("{} {}", r#type, r#match);
}

#[cfg(feature = "extra")]
pub fn cfg_gated() {}

pub fn construction() {
    let wrapped = Wrapper { inner: &1 };
    let optional = Some(1);
    let _ = (wrapped, optional);
}

// A union is not a struct and must never be reported as one.

pub union Number {
    integer: i32,
    float: f32,
}

// Foreign function signatures live inside an extern block scope. The ABI string
// is not a fact and nothing is resolved.

extern "C" {
    fn puts(message: *const i8) -> i32;
}

// Associated items in an inherent impl belong to the impl scope.

impl Number {
    pub const LIMIT: u32 = 1;
    pub type Word = u32;
}

// Generic parameters, lifetimes and `where` clauses are syntax, not
// declarations. `dyn`/`impl Trait` are not extracted as references.

pub fn generic_parameter<T: Clone>(value: T) -> T {
    value
}

pub fn dynamic_object(value: &dyn core::fmt::Debug) -> String {
    format!("{value:?}")
}

pub fn impl_trait_return() -> impl Iterator<Item = u32> {
    core::iter::empty()
}
