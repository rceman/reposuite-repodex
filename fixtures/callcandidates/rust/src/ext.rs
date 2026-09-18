extern "C" {
    fn ext_helper(x: i32) -> i32;
}
fn run() {
    ext_helper(1);
}
