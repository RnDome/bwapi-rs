use std::path::{Path, PathBuf};

/// A checkout of the OpenBW fork of BWAPI (`github.com/openbw/bwapi`).
const DIR_VAR: &str = "OPENBW_BWAPI_DIR";
/// Its built libraries; `$OPENBW_BWAPI_DIR/build/lib` by default.
const LIB_VAR: &str = "OPENBW_BWAPI_LIB_DIR";
/// Optional: a checkout of BWEM-community, built as the reference for the port.
const BWEM_VAR: &str = "BWEM_DIR";
/// Optional, with `BWEM_DIR`: a checkout of BWEB v1.17, built as the reference
/// for the port.
const BWEB_VAR: &str = "BWEB_DIR";
/// Optional: a checkout of FAP, built as the reference for the port.
const FAP_VAR: &str = "FAP_DIR";

fn main() {
    println!("cargo:rerun-if-env-changed={DIR_VAR}");
    println!("cargo:rerun-if-env-changed={LIB_VAR}");
    println!("cargo:rerun-if-changed=cpp");

    let Some(dir) = std::env::var_os(DIR_VAR).map(PathBuf::from) else {
        panic!(
            "{DIR_VAR} is not set: point it at a built checkout of github.com/openbw/bwapi \
             (libBWAPI.so, libBWAPILIB.so and libOpenBWData.so in build/lib)"
        );
    };
    let lib = std::env::var_os(LIB_VAR).map_or_else(|| dir.join("build/lib"), PathBuf::from);
    for name in ["libBWAPI.so", "libBWAPILIB.so", "libOpenBWData.so"] {
        if !lib.join(name).exists() {
            panic!(
                "{} has no {name}; build the fork or set {LIB_VAR}",
                lib.display()
            );
        }
    }

    // The include paths of BWAPILauncher, which reaches into GameImpl the same way.
    let src = dir.join("bwapi");
    let mut build = cc::Build::new();
    build.cpp(true).std("gnu++14").warnings(false);
    for include in [
        "include",
        "OpenBWData",
        "Util/Source",
        "BWAPI/Source/BWAPI",
        "BWAPI/Source",
        "BWAPICore",
        "Shared",
    ] {
        build.include(src.join(include));
    }
    build.file("cpp/host.cpp").compile("openbw_host");

    // BWEM-community in C++, the reference for the Rust port, when its sources
    // are at hand.
    println!("cargo:rerun-if-env-changed={BWEM_VAR}");
    println!("cargo::rustc-check-cfg=cfg(bwem_reference)");
    if let Some(bwem) = std::env::var_os(BWEM_VAR).map(PathBuf::from) {
        let bwem = bwem.join("BWEM");
        let mut sources: Vec<PathBuf> = std::fs::read_dir(bwem.join("src"))
            .unwrap_or_else(|e| {
                panic!(
                    "{BWEM_VAR}: cannot read {}: {e}",
                    bwem.join("src").display()
                )
            })
            .map(|e| e.expect("cannot read a directory entry").path())
            .filter(|p| p.extension().is_some_and(|e| e == "cpp"))
            .collect();
        sources.sort();
        // Against the BWAPI headers bots are built with, as `bwapi` is: inline
        // functions of the fork differ, `getApproxDistance` among them.
        let vendored = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap())
            .join("../bwapi/vendor/bwapi/bwapi/include");
        println!("cargo:rerun-if-changed={}", vendored.display());
        let mut reference = cc::Build::new();
        reference.cpp(true).std("c++17").warnings(false);
        reference
            .include(&vendored)
            .include(bwem.join("include"))
            .include(bwem.join("src"))
            .files(&sources)
            .file("cpp/bwem_reference.cpp");
        println!("cargo:rustc-cfg=bwem_reference");
        println!("cargo:rerun-if-changed={}", bwem.display());

        // BWEB in C++ on top of it, the reference for the port of BWEB, in the
        // same library: it calls into BWEM.
        if let Some(bweb) = std::env::var_os(BWEB_VAR).map(PathBuf::from) {
            add_bweb(&mut reference, &bweb.join("Source"));
        }
        reference.compile("bwem_reference");
    }
    println!("cargo:rerun-if-env-changed={BWEB_VAR}");
    println!("cargo::rustc-check-cfg=cfg(bweb_reference)");

    // FAP in C++, the reference for the port of FAP, when its sources are at
    // hand. Header only; against the BWAPI headers bots are built with.
    println!("cargo:rerun-if-env-changed={FAP_VAR}");
    println!("cargo::rustc-check-cfg=cfg(fap_reference)");
    if let Some(fap) = std::env::var_os(FAP_VAR).map(PathBuf::from) {
        let include = fap.join("FAP/include");
        let vendored = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap())
            .join("../bwapi/vendor/bwapi/bwapi/include");
        cc::Build::new()
            .cpp(true)
            .std("c++17")
            .warnings(false)
            .include(&vendored)
            .include(&include)
            .file("cpp/fap_reference.cpp")
            .compile("fap_reference");
        println!("cargo:rustc-cfg=fap_reference");
        println!("cargo:rerun-if-changed={}", include.display());
    }

    println!("cargo:rustc-link-search=native={}", lib.display());
    for name in ["BWAPI", "BWAPILIB", "OpenBWData"] {
        println!("cargo:rustc-link-lib=dylib={name}");
    }
    println!("cargo:rustc-link-lib=stdc++");
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", lib.display());
    // `bwapi` links its own BWAPILIB, as a bot does. In a bot that copy is
    // private to the module; in a test binary its symbols would be exported,
    // the host's libBWAPILIB.so would bind to them, and both copies would
    // destroy the same globals at exit.
    println!("cargo:rustc-link-arg=-Wl,--exclude-libs,ALL");
}

/// Adds BWEB from `source` and its reference shim to `build`. BWEB v1.17
/// builds only with MSVC, against the fork of BWEM in McRave, and reads and
/// writes past its grids on maps 256 tiles wide; the checkouts in `BWEB_DIR`
/// and `BWEM_DIR` carry local patches for these, marked "Local patch".
fn add_bweb(build: &mut cc::Build, source: &Path) {
    let header = std::fs::read_to_string(source.join("BWEB.h"))
        .unwrap_or_else(|e| panic!("{BWEB_VAR}: cannot read BWEB.h: {e}"));
    assert!(
        header.contains("Local patch"),
        "{BWEB_VAR}: BWEB v1.17 without the local patches"
    );
    let mut sources: Vec<PathBuf> = std::fs::read_dir(source)
        .unwrap_or_else(|e| panic!("{BWEB_VAR}: cannot read {}: {e}", source.display()))
        .map(|e| e.expect("cannot read a directory entry").path())
        .filter(|p| p.extension().is_some_and(|e| e == "cpp"))
        .collect();
    sources.sort();

    build
        .include(source)
        .files(&sources)
        .file("cpp/bweb_reference.cpp");
    println!("cargo:rustc-cfg=bweb_reference");
    println!("cargo:rerun-if-changed={}", source.display());
}
