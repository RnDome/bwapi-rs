use std::fs;
use std::path::{Path, PathBuf};

const BWAPI_INCLUDE: &str = "vendor/bwapi/bwapi/include";
const BWAPILIB: &str = "vendor/bwapi/bwapi/BWAPILIB";

fn main() {
    if !Path::new(BWAPI_INCLUDE).join("BWAPI.h").exists() {
        panic!("BWAPI headers not found in {BWAPI_INCLUDE}; run `git submodule update --init`");
    }
    let windows = std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows");

    let mut generated = cpp_files("cpp/generated");
    generated.sort();

    // On Linux the host provides BWAPILIB at dlopen. On Windows BWAPI.dll
    // does not export it: every AI module links its own copy.
    if windows {
        let mut sources = cpp_files(&format!("{BWAPILIB}/Source"));
        sources.push(Path::new(BWAPILIB).join("UnitCommand.cpp"));
        sources.sort();
        // svnrev.h as BWAPI's revisionUpdate.vbs writes it for the 4.4.0 release.
        let out = PathBuf::from(std::env::var("OUT_DIR").expect("cargo sets OUT_DIR"));
        fs::write(
            out.join("svnrev.h"),
            "#pragma once\nstatic const int SVN_REV = 5016;\n\n#include \"starcraftver.h\"\n",
        )
        .expect("cannot write svnrev.h");

        // BWAPI's code: built with its own settings, not under our warnings.
        let mut lib = cc::Build::new();
        lib.cpp(true).std("c++17").warnings(false).include(&out);
        include_bwapi(&mut lib);
        for (name, value) in [
            ("NOMINMAX", None),
            ("WIN32", None),
            ("_WIN32_WINNT", Some("0x0501")),
            ("NTDDI_VERSION", Some("0x05010300")),
            ("NDEBUG", None),
        ] {
            lib.define(name, value);
        }
        lib.files(&sources).compile("bwapilib");
        println!("cargo:rerun-if-changed={BWAPILIB}");
    }

    let mut glue = cc::Build::new();
    glue.cpp(true).std("c++17");
    include_bwapi(&mut glue);
    glue.warnings(true)
        .extra_warnings(true)
        .flag("-Wconversion")
        .flag("-Wsign-conversion")
        .warnings_into_errors(true)
        .file("cpp/runtime.cpp")
        .file("cpp/sets.cpp")
        .file("cpp/aimodule.cpp")
        .file("cpp/event.cpp")
        .files(&generated)
        .compile("bwapi_c");

    println!("cargo:rerun-if-changed=cpp");
    println!("cargo:rerun-if-changed={BWAPI_INCLUDE}");
}

/// BWAPI headers as system headers: their warnings must not break `-Werror`.
fn include_bwapi(build: &mut cc::Build) {
    if build.get_compiler().is_like_msvc() {
        build.flag("/imsvc").flag(BWAPI_INCLUDE);
    } else {
        build.flag(format!("-isystem{BWAPI_INCLUDE}"));
    }
}

fn cpp_files(dir: &str) -> Vec<PathBuf> {
    fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("cannot read {dir}: {e}"))
        .map(|e| e.expect("cannot read a directory entry").path())
        .filter(|p| p.extension().is_some_and(|e| e == "cpp"))
        .collect()
}
