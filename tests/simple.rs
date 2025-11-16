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
    let id = buildid::build_id().unwrap();
    assert!(!id.is_empty());

    println!("{}", hex::encode(id));
    if let Some(expected_id) = expected_build_id() {
        assert_eq!(expected_id, id);
    }
}

#[cfg(all(target_family = "unix", target_vendor = "apple",))]
mod mach {
    fn otool_uuid(exe_path: &std::path::Path) -> Option<Vec<u8>> {
        use std::process::Command;
        let otool_l = Command::new("otool")
            .arg("-l")
            .arg(exe_path)
            .output()
            .unwrap();
        let mut lines = otool_l.stdout.split(|f| f == &b'\n');
        while let Some(line) = lines.next() {
            if line == b"     cmd LC_UUID" {
                // should be " cmdsize 24", then the uuid
                lines.next().expect("expected cmdsize line");
                let uuid_line = std::str::from_utf8(lines.next().expect("expected uuid line"))
                    .expect("invalid utf8");

                let parts: Vec<&str> = uuid_line.trim().split_whitespace().collect();
                assert!(parts.len() == 2 && parts[0] == "uuid");
                let uuid_str = parts[1].replace("-", "");
                return Some(hex::decode(uuid_str).unwrap());
            }
        }
        None
    }

    #[test]
    fn has_build_id_mach() {
        let exe_path = std::env::current_exe().unwrap();
        let uuid = otool_uuid(exe_path.as_ref()).unwrap();

        let id = buildid::build_id().unwrap();
        assert_eq!(uuid, id);
    }
}
