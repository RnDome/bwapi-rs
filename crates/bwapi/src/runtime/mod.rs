//! The hand-written side of the C++ boundary: ABI values and the buffer protocol
//! (ffi.rs), BWAPI callbacks as `BWAPI::Event` (event.rs), sets without copying
//! (sets.rs), Brood War strings (string.rs), and the shapes of generated types
//! (types.rs).

pub(crate) mod event;
pub(crate) mod ffi;
pub(crate) mod sets;
pub(crate) mod string;
pub(crate) mod types;
