use std::ffi::OsString;
use std::path::{Path, PathBuf};
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
        #[cfg(not(all(target_os = "windows", target_env = "msvc")))]
        let root = std::env::var_os("CARGO_TARGET_TMPDIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target"))
            .join(unique);

        // Keep MSVC link-repro paths below its MAX_PATH limit.
        #[cfg(all(target_os = "windows", target_env = "msvc"))]
        let root = std::env::temp_dir().join(unique);
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
            .arg("rustc")
            .args(["--example", "incremental_probe"])
            .arg("--quiet")
            .arg("--offline")
            .env("CARGO_INCREMENTAL", "1")
            .env("CARGO_TARGET_DIR", self.0.join("target"))
            .env_remove("RUSTC_WRAPPER");

        // CI disables dev debug information, and rustc normally requests
        // /OPT:REF. Both can prevent the incremental MSVC link this test needs.
        // Apply linker options only to the probe, preserving inherited Rust flags.
        #[cfg(all(target_os = "windows", target_env = "msvc"))]
        {
            let repro = self.0.join("repro");
            std::fs::create_dir_all(&repro).unwrap();
            cargo
                .env("CARGO_PROFILE_DEV_DEBUG", "2")
                .args([
                    "--",
                    "-C",
                    "linker=link.exe",
                    "-C",
                    "link-arg=/DEBUG:FULL",
                    "-C",
                    "link-arg=/INCREMENTAL",
                    "-C",
                    "link-arg=/OPT:NOREF",
                    "-C",
                    "link-arg=/OPT:NOICF",
                    "-C",
                ])
                .arg(format!("link-arg=/LINKREPRO:{}", repro.display()));
        }
        assert_success("cargo rustc", cargo.output().unwrap());

        let executable = self
            .0
            .join("target/debug/examples/incremental_probe")
            .with_extension(std::env::consts::EXE_EXTENSION);
        read_id(&executable, None)
    }
}

impl Drop for TestProject {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn read_id(executable: &Path, marker: Option<&str>) -> String {
    let output = Command::new(executable).output().unwrap();
    assert_success("test executable", output.clone());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let mut lines = stdout.lines();
    let id = lines.next().unwrap().to_owned();
    if let Some(marker) = marker {
        assert_eq!(lines.next(), Some(marker), "probe did not change");
    }
    id
}

#[cfg(all(target_os = "windows", target_env = "msvc"))]
mod msvc {
    use super::*;

    pub fn incremental_ids(project: &TestProject) -> (String, String) {
        const FIRST: &str = "buildid-incremental-marker-first";
        const OTHER: &str = "buildid-incremental-marker-other";
        project.write_source(FIRST);
        project.build_id();

        // Cargo removes the executable before relinking, and rustc changes
        // temporary object paths. Replay MSVC's captured inputs directly so
        // both links use the same objects, executable, PDB and ILK database.
        let repro = project.0.join("repro");
        let mut files = Vec::new();
        collect_files(&repro, &mut files);
        let response = files
            .iter()
            .find(|path| path.file_name().unwrap() == "link.rsp")
            .cloned()
            .unwrap_or_else(|| panic!("MSVC did not capture link.rsp: {files:?}"));
        let executable = repro.join("probe.exe");
        let link = || {
            let mut linker = find_msvc_tools::find(std::env::consts::ARCH, "link.exe")
                .expect("MSVC linker not found");
            linker
                .current_dir(&repro)
                .arg(format!("@{}", response.display()))
                .args(["/OUT:probe.exe", "/PDB:probe.pdb", "/ILK:probe.ilk"]);
            assert_success("MSVC link replay", linker.output().unwrap());
        };
        link();
        let first = read_id(&executable, Some(FIRST));

        // Change only the string literal in the captured COFF objects. Its
        // size and relocations stay fixed, allowing a true incremental link.
        let mut replacements = 0;
        for path in files {
            if !matches!(path.extension().and_then(|s| s.to_str()), Some("o" | "obj")) {
                continue;
            }
            let mut data = std::fs::read(&path).unwrap();
            let mut changed = false;
            for offset in 0..data.len().saturating_sub(FIRST.len() - 1) {
                if &data[offset..offset + FIRST.len()] == FIRST.as_bytes() {
                    data[offset..offset + FIRST.len()].copy_from_slice(OTHER.as_bytes());
                    replacements += 1;
                    changed = true;
                }
            }
            if changed {
                std::fs::write(path, data).unwrap();
            }
        }
        assert!(
            replacements > 0,
            "probe literal missing from captured objects"
        );
        link();
        let second = read_id(&executable, Some(OTHER));
        (first, second)
    }

    fn collect_files(directory: &Path, files: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                collect_files(&path, files);
            } else {
                files.push(path);
            }
        }
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
    #[cfg(all(target_os = "windows", target_env = "msvc"))]
    let (first, second) = msvc::incremental_ids(&project);
    #[cfg(not(all(target_os = "windows", target_env = "msvc")))]
    let (first, second) = {
        project.write_source("first");
        let first = project.build_id();
        project.write_source("other");
        (first, project.build_id())
    };

    assert_incremental_link_was_used(&first, &second);
    assert_ne!(first, second, "build ID did not change after rebuilding");
}
