mod left {
    fn helper() {}
    fn run() {
        helper();
    }
}
mod right {
    fn helper() {}
}
mod lonely_mod {
    fn run() {
        helper();
    }
}
fn file_helper() {}
fn file_run() {
    file_helper();
}
