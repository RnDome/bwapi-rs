//! Values that cross the C ABI of cpp/include/bwapi_c.h, and the strings and vectors
//! that C++ returns in a `Slice`.
//!
//! A thunk returning `n` elements of a C type `T` fills its `Slice` with memory of
//! `bwapi_c_alloc_slice(n, sizeof(T), alignof(T))`, which is the global allocator
//! with `Layout::array::<E>(n)` for the Rust mirror `E` of `T`. So the slice is the
//! allocation of a `Vec<E>` with length and capacity `n`, and `Slice::into_vec`
//! takes it over without copying.

use core::alloc::Layout;
use core::ffi::c_void;
use core::ptr;

use crate::{Position, TilePosition, WalkPosition};

/// `Position`, `TilePosition` and `WalkPosition` of bwapi_c.h: the same layout.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct Pos {
    pub x: i32,
    pub y: i32,
}

/// `Pair` of bwapi_c.h.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct PairI32 {
    pub first: i32,
    pub second: i32,
}

/// `SetCursor` of bwapi_c.h: room for a C++ set iterator, moved by copying bytes.
#[repr(C)]
pub(crate) struct SetCursor {
    _words: [u64; 4],
}

/// `Slice` of bwapi_c.h: a string or a vector a thunk returns.
#[repr(C)]
pub(crate) struct Slice {
    data: *mut c_void,
    len: usize,
}

impl Slice {
    pub(crate) fn new() -> Self {
        Slice {
            data: ptr::null_mut(),
            len: 0,
        }
    }

    /// # Safety
    ///
    /// A thunk has filled the slice with elements of the C type that `E` mirrors.
    pub(crate) unsafe fn into_vec<E>(self) -> Vec<E> {
        if self.len == 0 {
            return Vec::new();
        }
        // SAFETY: the allocation of a `Vec<E>` of `len` elements, all written by
        // the thunk (see the module documentation).
        unsafe { Vec::from_raw_parts(self.data.cast(), self.len, self.len) }
    }
}

/// The memory of a `Slice`: the global allocator, as a `Vec` allocates it.
#[unsafe(no_mangle)]
extern "C" fn bwapi_c_alloc_slice(
    count: usize,
    elem_size: usize,
    elem_align: usize,
) -> *mut c_void {
    let layout = elem_size
        .checked_mul(count)
        .filter(|&size| size > 0)
        .and_then(|size| Layout::from_size_align(size, elem_align).ok());
    let Some(layout) = layout else {
        eprintln!(
            "bwapi-rs: a thunk asked for a slice of {count} x {elem_size} bytes aligned to {elem_align}"
        );
        std::process::abort();
    };
    // SAFETY: the size is not zero.
    let data = unsafe { std::alloc::alloc(layout) };
    if data.is_null() {
        std::alloc::handle_alloc_error(layout);
    }
    data.cast()
}

macro_rules! pos_conversions {
    ($($t:ident),*) => {$(
        impl From<Pos> for $t {
            fn from(p: Pos) -> Self {
                $t { x: p.x, y: p.y }
            }
        }

        impl From<$t> for Pos {
            fn from(p: $t) -> Self {
                Pos { x: p.x, y: p.y }
            }
        }
    )*};
}

pos_conversions!(Position, TilePosition, WalkPosition);
