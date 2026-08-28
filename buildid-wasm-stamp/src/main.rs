use std::env;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::Path;
use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "\
Stamp a final WebAssembly module or component with a guest-readable build ID.

Usage: buildid-wasm-stamp [--check | --id-source SOURCE] [-q] [-o OUTPUT] INPUT

Options:
    --id-source SOURCE         Use top-level or content [default: top-level]
    --check                    Validate the existing stamp without writing
    -o, --output OUTPUT        Write to OUTPUT instead of replacing INPUT
    -q, --quiet                Do not print the build ID
    -h, --help                 Print this help
    -V, --version              Print the version
";

struct Options {
    input: PathBuf,
    output: Option<PathBuf>,
    check: bool,
    mode: StampMode,
    quiet: bool,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum StampMode {
    TopLevel,
    Generate,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("buildid-wasm-stamp: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let Some(options) = parse_options()? else {
        return Ok(());
    };
    let input = fs::read(&options.input)
        .map_err(|error| format!("failed to read {}: {error}", options.input.display()))?;

    if options.check {
        let id = buildid_wasm_stamp::check(&input).map_err(|error| error.to_string())?;
        if !options.quiet {
            println!("{}", hex(&id));
        }
        return Ok(());
    }

    let stamped = match options.mode {
        StampMode::TopLevel => buildid_wasm_stamp::stamp_top_level(&input),
        StampMode::Generate => buildid_wasm_stamp::stamp(&input),
    }
    .map_err(|error| error.to_string())?;
    let output = options.output.as_ref().unwrap_or(&options.input);
    if stamped.wasm != input || output != &options.input {
        atomic_write(output, &stamped.wasm)
            .map_err(|error| format!("failed to write {}: {error}", output.display()))?;
    }
    if !options.quiet {
        println!("{}", hex(&stamped.id));
    }
    Ok(())
}

fn parse_options() -> Result<Option<Options>, String> {
    let mut args = env::args_os().skip(1);
    let mut input = None;
    let mut output = None;
    let mut check = false;
    let mut mode = None;
    let mut quiet = false;

    while let Some(argument) = args.next() {
        match argument.to_str() {
            Some("-h" | "--help") => {
                print!("{USAGE}");
                return Ok(None);
            }
            Some("-V" | "--version") => {
                println!("buildid-wasm-stamp {}", env!("CARGO_PKG_VERSION"));
                return Ok(None);
            }
            Some("--check") => check = true,
            Some("--id-source") => {
                let source = args
                    .next()
                    .ok_or_else(|| "--id-source requires top-level or content".to_owned())?;
                let source = source
                    .to_str()
                    .ok_or_else(|| "ID source must be valid UTF-8".to_owned())?;
                set_mode(&mut mode, parse_id_source(source)?)?;
            }
            Some(value) if value.starts_with("--id-source=") => {
                set_mode(&mut mode, parse_id_source(&value["--id-source=".len()..])?)?
            }
            Some("-q" | "--quiet") => quiet = true,
            Some("-o" | "--output") => {
                let path = args
                    .next()
                    .ok_or_else(|| "--output requires a path".to_owned())?;
                if output.replace(PathBuf::from(path)).is_some() {
                    return Err("--output may only be specified once".to_owned());
                }
            }
            Some(value) if value.starts_with('-') => {
                return Err(format!("unknown option: {value}\n\n{USAGE}"));
            }
            _ => {
                if input.replace(PathBuf::from(argument)).is_some() {
                    return Err(format!("expected one input file\n\n{USAGE}"));
                }
            }
        }
    }

    let input = input.ok_or_else(|| format!("missing input file\n\n{USAGE}"))?;
    if check && output.is_some() {
        return Err("--check cannot be combined with --output".to_owned());
    }
    if check && mode.is_some() {
        return Err("--check cannot be combined with a stamping mode".to_owned());
    }
    Ok(Some(Options {
        input,
        output,
        check,
        mode: mode.unwrap_or(StampMode::TopLevel),
        quiet,
    }))
}

fn atomic_write(path: &Path, contents: &[u8]) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temporary = tempfile::Builder::new()
        .prefix(".buildid-wasm-stamp-")
        .tempfile_in(parent)?;

    if let Ok(metadata) = fs::metadata(path) {
        temporary
            .as_file()
            .set_permissions(metadata.permissions())?;
    }
    temporary.write_all(contents)?;
    temporary.flush()?;
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|error| error.error)?;

    if let Ok(directory) = File::open(parent) {
        directory.sync_all()?;
    }
    Ok(())
}

fn parse_id_source(source: &str) -> Result<StampMode, String> {
    match source {
        "top-level" => Ok(StampMode::TopLevel),
        "content" => Ok(StampMode::Generate),
        value => Err(format!(
            "invalid ID source {value:?}; expected top-level or content"
        )),
    }
}

fn set_mode(mode: &mut Option<StampMode>, requested: StampMode) -> Result<(), String> {
    if mode.is_some_and(|current| current != requested) {
        return Err("conflicting ID sources; select either top-level or content".to_owned());
    }
    *mode = Some(requested);
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(DIGITS[usize::from(byte >> 4)]));
        output.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    output
}
