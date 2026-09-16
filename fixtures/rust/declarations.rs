//! Fixture: Rust declarations and lexical containment.

mod inline {
    pub mod nested {
        pub fn deep() {}
    }

    pub struct Inner;
}

mod external;

pub struct Config {
    pub name: String,
    count: u32,
}

pub enum State {
    Idle,
    Running(u32),
    Failed { code: i32 },
}

pub trait Handler {
    fn handle(&self, value: u32) -> u32;

    fn label(&self) -> String {
        String::from("handler")
    }
}

pub type Outcome<T> = core::result::Result<T, Error>;

pub const MAX_ITEMS: usize = 10;

pub static mut COUNTER: u64 = 0;

pub fn free_function(value: u32) -> u32 {
    value
}

fn outer() {
    fn nested_function() {}

    let closure = |x: u32| x + 1;
    nested_function();
    closure(1);
}
