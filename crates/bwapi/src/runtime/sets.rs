//! BWAPI sets without copying, over the set functions of cpp/include/bwapi_c.h.
//!
//! - `XSet<'game>`: a live set that BWAPI returns by reference. Its address is
//!   stable for the whole match, so the descriptor can be stored. Its contents
//!   change between frames, so every read takes the callback `Context`, and the
//!   iterator `XSetIter<'frame, 'game>` cannot outlive the callback.
//! - `XQuery<'game>`: a set that BWAPI returns by value. It lives in the C++ heap,
//!   is freed on drop, and is itself the iterator; it needs no `Context`.
//!
//! Iteration protocol: `at_end` reports exhaustion; `next` requires `!at_end`.
//! `Iterator::next` is the only caller of `next` and checks `at_end` right before it,
//! so exhaustion never depends on the value of an element: a null element panics.

use core::fmt;
use core::iter::FusedIterator;
use core::marker::{PhantomData, PhantomPinned};
use core::mem::MaybeUninit;
use core::ptr::NonNull;

use super::ffi::SetCursor;
use crate::generated::raw;
use crate::session::{Brand, Context, FrameToken};
use crate::{Bullet, Force, Player, Region, Unit};

macro_rules! define_sets {
    (
        $elem:ident, $cpp:literal, $c_set:literal, $c_query:literal,
        $set:ident, $iter:ident, $query:ident,
        $raw_set:ident, $raw_query:ident, $ffi:ident
    ) => {
        /// Opaque C++ set.
        #[repr(C)]
        pub(crate) struct $raw_set {
            _opaque: [u8; 0],
            _marker: PhantomData<(*mut u8, PhantomPinned)>,
        }

        /// Opaque `bwapi_c::QueryState` over a C++ set.
        #[repr(C)]
        pub(crate) struct $raw_query {
            _opaque: [u8; 0],
            _marker: PhantomData<(*mut u8, PhantomPinned)>,
        }

        mod $ffi {
            use super::*;

            // The set functions of cpp/include/bwapi_c.h.
            unsafe extern "C" {
                #[link_name = concat!($c_set, "_size")]
                pub(super) fn set_len(set: *const $raw_set) -> usize;
                #[link_name = concat!($c_set, "_contains")]
                pub(super) fn set_contains(set: *const $raw_set, elem: *mut raw::$elem) -> bool;
                #[link_name = concat!($c_set, "_begin")]
                pub(super) fn set_iter(set: *const $raw_set, cursor: *mut SetCursor);
                #[link_name = concat!($c_set, "Cursor_atEnd")]
                pub(super) fn set_at_end(cursor: *const SetCursor) -> bool;
                #[link_name = concat!($c_set, "Cursor_next")]
                pub(super) fn set_next(cursor: *mut SetCursor) -> *mut raw::$elem;
                #[link_name = concat!($c_set, "Cursor_remaining")]
                pub(super) fn set_remaining(cursor: *const SetCursor) -> usize;
                #[link_name = concat!($c_query, "_atEnd")]
                pub(super) fn query_at_end(query: *const $raw_query) -> bool;
                #[link_name = concat!($c_query, "_next")]
                pub(super) fn query_next(query: *mut $raw_query) -> *mut raw::$elem;
                #[link_name = concat!($c_query, "_remaining")]
                pub(super) fn query_remaining(query: *const $raw_query) -> usize;
                #[link_name = concat!($c_query, "_release")]
                pub(super) fn query_drop(query: *mut $raw_query);
            }
        }

        #[doc = concat!("A live `", $cpp, "` owned by the game.")]
        ///
        /// The descriptor can be stored for the whole match; reading the contents
        /// takes the `Context` of the running callback.
        #[derive(Clone, Copy)]
        pub struct $set<'game> {
            ptr: NonNull<$raw_set>,
            brand: Brand<'game>,
        }

        impl<'game> $set<'game> {
            /// # Safety
            ///
            /// `ptr` must point to a set of this match that stays at this address
            /// for the whole match.
            #[allow(dead_code)]
            pub(crate) unsafe fn from_raw(brand: Brand<'game>, ptr: *const $raw_set) -> Self {
                $set {
                    ptr: NonNull::new(ptr.cast_mut()).expect("BWAPI returned a null set"),
                    brand,
                }
            }

            pub fn len(self, cx: Context<'_, 'game>) -> usize {
                let _ = cx;
                unsafe { $ffi::set_len(self.ptr.as_ptr()) }
            }

            pub fn is_empty(self, cx: Context<'_, 'game>) -> bool {
                self.len(cx) == 0
            }

            pub fn contains(self, cx: Context<'_, 'game>, elem: $elem<'game>) -> bool {
                let _ = cx;
                unsafe { $ffi::set_contains(self.ptr.as_ptr(), elem.as_ptr()) }
            }

            pub fn iter<'frame>(self, cx: Context<'frame, 'game>) -> $iter<'frame, 'game> {
                let _ = cx;
                let mut cursor = MaybeUninit::<SetCursor>::uninit();
                // SAFETY: `set_iter` constructs the cursor in place.
                unsafe {
                    $ffi::set_iter(self.ptr.as_ptr(), cursor.as_mut_ptr());
                    $iter {
                        cursor: cursor.assume_init(),
                        brand: self.brand,
                        _frame: PhantomData,
                    }
                }
            }
        }

        impl PartialEq for $set<'_> {
            fn eq(&self, other: &Self) -> bool {
                self.ptr == other.ptr
            }
        }

        impl Eq for $set<'_> {}

        impl fmt::Debug for $set<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, concat!(stringify!($set), "({:p})"), self.ptr)
            }
        }

        #[doc = concat!("Iterator over a live `", $cpp, "`; valid during one callback.")]
        pub struct $iter<'frame, 'game> {
            cursor: SetCursor,
            brand: Brand<'game>,
            _frame: PhantomData<&'frame FrameToken>,
        }

        impl<'game> Iterator for $iter<'_, 'game> {
            type Item = $elem<'game>;

            fn next(&mut self) -> Option<$elem<'game>> {
                if unsafe { $ffi::set_at_end(&self.cursor) } {
                    return None;
                }
                // Precondition of `set_next`: not at the end, checked right above.
                let ptr = unsafe { $ffi::set_next(&mut self.cursor) };
                Some(unsafe { $elem::from_raw(self.brand, ptr) }.expect("BWAPI returned a null element"))
            }

            fn size_hint(&self) -> (usize, Option<usize>) {
                let n = unsafe { $ffi::set_remaining(&self.cursor) };
                (n, Some(n))
            }
        }

        impl ExactSizeIterator for $iter<'_, '_> {}

        impl FusedIterator for $iter<'_, '_> {}

        #[doc = concat!("A `", $cpp, "` returned by value. It lives in the C++ heap and is freed on drop.")]
        pub struct $query<'game> {
            ptr: NonNull<$raw_query>,
            brand: Brand<'game>,
        }

        impl<'game> $query<'game> {
            /// # Safety
            ///
            /// `ptr` must come from a thunk of this match that returned a new query.
            #[allow(dead_code)]
            pub(crate) unsafe fn from_raw(brand: Brand<'game>, ptr: *mut $raw_query) -> Self {
                $query {
                    ptr: NonNull::new(ptr).expect("C++ `new` returned null"),
                    brand,
                }
            }
        }

        impl<'game> Iterator for $query<'game> {
            type Item = $elem<'game>;

            fn next(&mut self) -> Option<$elem<'game>> {
                if unsafe { $ffi::query_at_end(self.ptr.as_ptr()) } {
                    return None;
                }
                // Precondition of `query_next`: not at the end, checked right above.
                let ptr = unsafe { $ffi::query_next(self.ptr.as_ptr()) };
                Some(unsafe { $elem::from_raw(self.brand, ptr) }.expect("BWAPI returned a null element"))
            }

            fn size_hint(&self) -> (usize, Option<usize>) {
                let n = unsafe { $ffi::query_remaining(self.ptr.as_ptr()) };
                (n, Some(n))
            }
        }

        impl ExactSizeIterator for $query<'_> {}

        impl FusedIterator for $query<'_> {}

        impl Drop for $query<'_> {
            fn drop(&mut self) {
                unsafe { $ffi::query_drop(self.ptr.as_ptr()) }
            }
        }

        impl fmt::Debug for $query<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.debug_struct(stringify!($query))
                    .field("remaining", &self.len())
                    .finish()
            }
        }
    };
}

define_sets! { Unit, "BWAPI::Unitset", "Unitset", "UnitQuery", UnitSet, UnitSetIter, UnitQuery, RawUnitSet, RawUnitQuery, unit_ffi }
define_sets! { Player, "BWAPI::Playerset", "Playerset", "PlayerQuery", PlayerSet, PlayerSetIter, PlayerQuery, RawPlayerSet, RawPlayerQuery, player_ffi }
define_sets! { Force, "BWAPI::Forceset", "Forceset", "ForceQuery", ForceSet, ForceSetIter, ForceQuery, RawForceSet, RawForceQuery, force_ffi }
define_sets! { Bullet, "BWAPI::Bulletset", "Bulletset", "BulletQuery", BulletSet, BulletSetIter, BulletQuery, RawBulletSet, RawBulletQuery, bullet_ffi }
define_sets! { Region, "BWAPI::Regionset", "Regionset", "RegionQuery", RegionSet, RegionSetIter, RegionQuery, RawRegionSet, RawRegionQuery, region_ffi }
