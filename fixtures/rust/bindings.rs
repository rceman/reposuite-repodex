//! Local name-binding coverage: `let`, parameters, closures, `for`, `match`,
//! `if let`, `while let`, destructuring, shadowing and wildcards.

fn module_helper() {}

fn params(a: u8, (x, y): (u8, u8), Point { px, py }: Point, mut counter: u32) {
    a();
    x();
}

fn lets() {
    let helper = || {};
    helper();

    let (left, right) = pair();
    let Point { x, y } = origin();
    let S { f1: renamed, f2 } = source();
    let Some(v) = maybe() else { return };
    let _ignored = compute();
    let _ = dropped();
    let mut growing = 0;
    let reference = &target;

    helper();

    {
        let helper = || {};
        helper();
    }

    helper();
}

fn closures() {
    let run = |arg| arg();
    let pair = |(a, b)| a();
    let typed = |value: u32| value;
    let nested = |outer| move |inner| inner + outer;
}

fn loops() {
    for item in items() {
        item();
    }
    for (key, value) in entries() {
        key();
        value();
    }
    for _ in once() {}
}

fn matching(input: Input) {
    match input {
        Some(found) if ready(found) => found(),
        Point { x, y } => x(),
        name @ Pattern::Deep => name(),
        None => fallback(),
        _ => fallback(),
    }
}

fn conditional() {
    if let Some(got) = maybe() {
        got();
    } else {
        missing();
    }
    if let Ok(a) = first() && let Ok(b) = second() {
        a();
        b();
    }
    while let Ok(next) = advance() {
        next();
    }
}

fn shadowed() {
    let helper = first();
    helper();
    let helper = second();
    helper();
}
