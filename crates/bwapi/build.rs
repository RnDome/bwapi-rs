use std::path::Path;

const BWAPI_INCLUDE: &str = "vendor/bwapi/bwapi/include";

fn main() {
    if !Path::new(BWAPI_INCLUDE).join("BWAPI.h").exists() {
        panic!("BWAPI headers not found in {BWAPI_INCLUDE}; run `git submodule update --init`");
    }

    // BWAPILIB is neither compiled nor linked: its symbols come from the
    // OpenBW host at dlopen.
    cc::Build::new()
        .cpp(true)
        .std("c++17")
        // -isystem: warnings inside BWAPI headers must not break -Werror.
        .flag(format!("-isystem{BWAPI_INCLUDE}"))
        .warnings(true)
        .extra_warnings(true)
        .flag("-Wconversion")
        .flag("-Wsign-conversion")
        .warnings_into_errors(true)
        .file("cpp/runtime.cpp")
        .compile("bwapi_c");

    println!("cargo:rerun-if-changed=cpp");
    println!("cargo:rerun-if-changed={BWAPI_INCLUDE}");
}
