extern crate bwapi_sys;

pub mod position;

pub mod string;
pub mod iterator;

pub mod player;
pub mod game;
pub mod unit;
pub mod region;

pub mod aimodule;

#[cfg(test)]
#[allow(non_snake_case)]
#[no_mangle]
pub extern "C" fn _Unwind_Resume() -> ! {
    use std::process;
    process::abort();
}

#[cfg(test)]
#[allow(non_snake_case)]
#[no_mangle]
pub extern "C" fn _Unwind_RaiseException() -> ! {
    use std::process;
    process::abort();
}
