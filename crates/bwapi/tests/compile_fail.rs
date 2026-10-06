//! The brand `'game` and the frame `'frame` reject what would be unsound.
//!
//! Each file in tests/ui must fail to compile with the error in its `.stderr`.
//! The positive control, which must compile, is examples/bot_rust.
//!
//! Compiler messages change between Rust versions; the `.stderr` files follow the
//! toolchain of rust-toolchain.toml (`TRYBUILD=overwrite cargo test -p bwapi --test compile_fail`).

#[test]
fn compile_fail() {
    trybuild::TestCases::new().compile_fail("tests/ui/*.rs");
}
