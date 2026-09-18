fn helper() {}
struct User(u64);
struct S;
impl S {
    fn method(&self) {}
    fn caller(&self) {
        helper();
    }
}
fn run() {
    let closure = || {};
    closure();
    let f = helper;
    f();
    helper();
    User(1);
}
