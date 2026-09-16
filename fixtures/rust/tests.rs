//! Fixture: Rust test evidence.

#[test]
fn test_something() {}

#[test]
#[should_panic]
#[ignore]
fn test_panics() {}

#[cfg(test)]
fn not_a_test_by_attribute() {}

fn test_named_without_attribute() {}

#[tokio::test]
async fn test_async() {}
