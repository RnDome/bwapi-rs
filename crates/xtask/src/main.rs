//! `cargo xtask generate`: regenerates the bwapi crate's bindings from `bwapi.api`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const USAGE: &str = "usage: cargo xtask generate";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["generate"] => match generate() {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        },
        _ => {
            eprintln!("{USAGE}");
            ExitCode::FAILURE
        }
    }
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crates/xtask lives two levels below the workspace root")
        .to_path_buf()
}

fn generate() -> Result<(), String> {
    let root = root();
    let manifest = root.join("bwapi.api");
    let src = fs::read_to_string(&manifest).map_err(|e| format!("{}: {e}", manifest.display()))?;
    let out = codegen::generate("bwapi.api", &src)?;

    let crate_dir = root.join("crates/bwapi");
    let include_dir = crate_dir.join("cpp/include");
    fs::create_dir_all(&include_dir).map_err(|e| format!("{}: {e}", include_dir.display()))?;
    let header = include_dir.join("bwapi_c.h");
    fs::write(&header, &out.header).map_err(|e| format!("{}: {e}", header.display()))?;
    let cpp_dir = crate_dir.join("cpp/generated");
    let rust_dir = crate_dir.join("src/generated");
    write_dir(&cpp_dir, &out.cpp)?;
    write_dir(&rust_dir, &out.rust)?;

    let rust_files: Vec<PathBuf> = out.rust.iter().map(|f| rust_dir.join(&f.path)).collect();
    let status = Command::new("rustfmt")
        .args(["--edition", "2024"])
        .args(&rust_files)
        .status()
        .map_err(|e| format!("rustfmt: {e}"))?;
    if !status.success() {
        return Err(format!("rustfmt failed: {status}"));
    }
    println!(
        "generated bwapi_c.h, {} C++ and {} Rust files",
        out.cpp.len(),
        out.rust.len()
    );
    Ok(())
}

/// Replaces the whole directory: files dropped from the manifest disappear too.
fn write_dir(dir: &Path, files: &[codegen::OutputFile]) -> Result<(), String> {
    if dir.exists() {
        fs::remove_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for f in files {
        let path = dir.join(&f.path);
        fs::write(&path, &f.contents).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(())
}
