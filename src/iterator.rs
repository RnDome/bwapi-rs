use bwapi_sys as sys;
use from_raw::FromRaw;
use std::marker::PhantomData;

/// Iterator is a wrapper over API iterator.
/// To ensure safety it's lifetime is bound
/// to the lifetime of the referenced data.
pub struct BwIterator<'i, 'g: 'i, T: FromRaw + 'g> {
    raw: &'i mut sys::Iterator,
    phantom: PhantomData<&'g T>,
}

impl<'i, 'g: 'i, T: FromRaw + 'g> BwIterator<'i, 'g, T> {
    pub unsafe fn from(raw: *mut sys::Iterator) -> BwIterator<'i, 'g, T> {
        assert!(!raw.is_null());
        BwIterator { raw: &mut *raw, phantom: PhantomData }
    }
}

impl<'i,'g: 'i, T: FromRaw + 'g> Iterator for BwIterator<'i, 'g, T> {
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

impl<'i, 'g: 'i, T: FromRaw + 'g> Drop for BwIterator<'i, 'g, T> {
    fn drop(&mut self) {
        unsafe {
            sys::Iterator_release(self.raw);
        }
    }
}
