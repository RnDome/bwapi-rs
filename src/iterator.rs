
use bwapi_sys::bridge as sys;
use std::marker::PhantomData;
use std::os::raw::c_void as void;

/// `FromRaw` is a trait for entities that
/// are typically created outside of Rust code.
/// TODO Move to a proper place
pub trait FromRaw {
    /// Construct entity from raw data. Unsafe.
    /// Please be 100% sure that you pass correct pointer.
    unsafe fn from_raw(raw: *mut void) -> Self;
}

/// Iterator is a wrapper over API iterator.
/// To ensure safety it's lifetime is bound
/// to the lifetime of the referenced data.
pub struct BwIterator<'iter, T: FromRaw + 'iter> {
    raw: &'iter mut sys::Iterator,
    phantom: PhantomData<&'iter T>,
}

impl<'iter, T: FromRaw + 'iter> BwIterator<'iter, T> {
    pub unsafe fn from(raw: *mut sys::Iterator) -> BwIterator<'iter, T> {
        assert!(!raw.is_null());
        BwIterator { raw: &mut *raw, phantom: PhantomData }
    }
}

impl<'iter, T: FromRaw + 'iter> Iterator for BwIterator<'iter, T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        unsafe {
            if sys::Iterator_valid(self.raw) {
                let item = sys::Iterator_get(self.raw);
                sys::Iterator_next(self.raw);
                Some(T::from_raw(item))
            } else {
                None
            }
        }
    }
}

impl<'iter, T: FromRaw + 'iter> Drop for BwIterator<'iter, T> {
    fn drop(&mut self) {
        unsafe {
            sys::Iterator_release(self.raw);
        }
    }
}
