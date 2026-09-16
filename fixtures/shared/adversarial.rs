// Fixture: shared adversarial cases for Rust.
//
// UTF-8 multibyte text before a declaration: "αβγ δεζ" and "日本語テキスト".

fn multibyte_after_utf8() {}

// Duplicate names in different contexts.

struct Duplicate;

impl Duplicate {
    fn name(&self) -> u32 {
        1
    }
}

mod nested {
    pub fn name() -> u32 {
        2
    }
}

// Deep nesting.

mod level_one {
    pub mod level_two {
        pub mod level_three {
            pub mod level_four {
                pub fn deepest() {}
            }
        }
    }
}

// Syntactically valid but semantically invalid: `Missing` is never declared.

fn semantically_invalid() -> Missing {
    let value: AlsoMissing = Missing::default();
    value
}

// Generated-looking source.

pub fn generated_0001() {}
pub fn generated_0002() {}
pub fn generated_0003() {}
pub fn generated_0004() {}
pub fn generated_0005() {}
pub fn generated_0006() {}
pub fn generated_0007() {}
pub fn generated_0008() {}
