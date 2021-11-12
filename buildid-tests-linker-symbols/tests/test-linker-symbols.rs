fn expected_build_id() -> Option<Vec<u8>> {
    let expected_build_id = std::env::var("BUILD_ID_TEST_EXPECTED");
    match expected_build_id {
        Ok(v) => Some(hex::decode(v).unwrap()),
        Err(std::env::VarError::NotPresent) => None,
        Err(e) => panic!("{}", e),
    }
}

#[test]
fn has_build_id() {
    env_logger::init();
    let id = buildid::build_id().unwrap();
    assert!(!id.is_empty());

    println!("{}", hex::encode(id));
    if let Some(expected_id) = expected_build_id() {
        assert_eq!(expected_id, id);
    }
}
