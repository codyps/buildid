#[test]
fn has_build_id() {
    assert!(!buildid::build_id().unwrap().is_empty());
}
