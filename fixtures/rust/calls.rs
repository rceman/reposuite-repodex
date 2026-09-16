//! Fixture: Rust call-shaped syntax.

fn callees() {
    plain(1, 2);
    module::path::qualified(3);
    value.method(4);
    Vec::new();
    generic_call::<u32>(5);
    let tuple = TupleStruct(6);
    let literal = StructLiteral { field: 7 };
    let closure = || 1;
    closure();
    (make_callable())(8);
    println!("{} {}", tuple, literal);
}
