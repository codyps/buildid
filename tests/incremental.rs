use std::ffi::OsString;
use std::path::PathBuf;
use std::process::{Command, Output};

struct TestProject(PathBuf);

impl TestProject {
    fn new() -> Self {
        let unique = format!(
            "buildid-incremental-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let root = std::env::var_os("CARGO_TARGET_TMPDIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target"))
            .join(unique);

        // Cargo links examples at a stable path, allowing link.exe to reuse
        // its incremental-link database between the two builds.
        std::fs::create_dir_all(root.join("examples")).unwrap();

        let dependency = env!("CARGO_MANIFEST_DIR").replace('\\', "/");
        std::fs::write(
            root.join("Cargo.toml"),
            format!(
                r#"[package]
name = "incremental-build-id-test"
version = "0.0.0"
edition = "2021"

[dependencies]
buildid = {{ path = "{dependency}" }}

[workspace]
"#,
            ),
        )
        .unwrap();

        Self(root)
    }

    fn write_source(&self, marker: &str) {
        std::fs::write(
            self.0.join("examples/incremental_probe.rs"),
            format!(
                r#"fn main() {{
    for byte in buildid::build_id().unwrap() {{
        print!("{{byte:02x}}");
    }}
    println!();
    println!("{marker}");
}}
"#,
            ),
        )
        .unwrap();
    }

    fn build_id(&self) -> String {
        let mut cargo = Command::new(cargo());
        cargo
            .current_dir(&self.0)
            .arg("build")
            .args(["--example", "incremental_probe"])
            .arg("--quiet")
            .arg("--offline")
            .env("CARGO_INCREMENTAL", "1")
            .env("CARGO_TARGET_DIR", self.0.join("target"))
            .env_remove("RUSTC_WRAPPER");
        assert_success("cargo build", cargo.output().unwrap());

        let executable = self
            .0
            .join("target/debug/examples/incremental_probe")
            .with_extension(std::env::consts::EXE_EXTENSION);
        let output = Command::new(executable).output().unwrap();
        assert_success("test executable", output.clone());

        String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .next()
            .unwrap()
            .to_owned()
    }
}

impl Drop for TestProject {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn cargo() -> OsString {
    std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"))
}

fn assert_success(description: &str, output: Output) {
    assert!(
        output.status.success(),
        "{description} failed with {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

#[cfg(all(target_os = "windows", target_env = "msvc"))]
fn assert_incremental_link_was_used(first: &str, second: &str) {
    assert_eq!(first.len(), 40);
    assert_eq!(second.len(), 40);
    assert_eq!(
        &first[..32],
        &second[..32],
        "PDB GUID changed, so the test did not exercise an incremental link"
    );
}

#[cfg(not(all(target_os = "windows", target_env = "msvc")))]
fn assert_incremental_link_was_used(_first: &str, _second: &str) {}

#[test]
fn build_id_changes_after_incremental_rebuild() {
    if std::env::var_os("BUILD_ID_TEST_EXPECTED").is_some() {
        eprintln!("skipping incremental rebuild test with a fixed build ID");
        return;
    }

    let project = TestProject::new();
    project.write_source("first");
    let first = project.build_id();

    project.write_source("second");
    let second = project.build_id();

    assert_incremental_link_was_used(&first, &second);
    assert_ne!(first, second, "build ID did not change after rebuilding");
}
