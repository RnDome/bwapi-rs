extern crate bwapi_sys;

fn main() {
    unsafe { bwapi_sys::BWAPIC_getClient() };
}
