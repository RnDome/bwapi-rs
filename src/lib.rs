extern crate bwapi_sys;

pub mod aimodule;
pub mod from_raw;

pub mod position;
pub mod color;

pub mod string;
pub mod iterator;
pub mod player;
pub mod game;
pub mod unit;
pub mod region;

#[cfg(all(test, windows))]
#[no_mangle]
pub extern "C" fn _Unwind_Resume() -> ! {
    use std::process;
    process::abort();
}

#[cfg(all(test, windows))]
#[no_mangle]
pub extern "C" fn _Unwind_RaiseException() -> ! {
    use std::process;
    process::abort();
}
