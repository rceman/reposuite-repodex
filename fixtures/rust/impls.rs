//! Fixture: Rust impl blocks, associated functions and receiver methods.

struct Widget;
struct Gadget;

trait Describe {
    fn describe(&self) -> String;

    fn kind(&self) -> &'static str {
        "unknown"
    }
}

impl Widget {
    fn new() -> Self {
        Widget
    }

    fn name(&self) -> &'static str {
        "widget"
    }
}

impl Gadget {
    fn name(&self) -> &'static str {
        "gadget"
    }
}

impl Describe for Widget {
    fn describe(&self) -> String {
        String::from(self.name())
    }
}

impl Describe for Gadget {
    fn describe(&self) -> String {
        String::from("gadget")
    }
}
