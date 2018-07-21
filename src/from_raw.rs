use std::os::raw::c_void as void;

/// `FromRaw` is a trait for entities that
/// are typically created outside of Rust code.
pub trait FromRaw {
    /// Construct entity from raw data. Unsafe.
    /// Please be 100% sure that you pass correct pointer.
    unsafe fn from_raw(raw: *mut void) -> Self;
}
