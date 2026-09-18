struct S;
impl S {
    fn assoc() {}
}
fn helper() {}
fn run() {
    crate::helper();
    self::helper();
    obj.method();
    S::assoc();
    (helper)();
    produce()();
    my_macro!();
}
