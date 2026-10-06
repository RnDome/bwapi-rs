//! The shapes of generated types; bwapi.api adds their methods.

/// A branded handle over a BWAPI object pointer.
macro_rules! handle {
    ($name:ident, $cpp:literal) => {
        #[doc = concat!("BWAPI: `", $cpp, "`")]
        ///
        /// A handle of the match `'game`: valid until the match ends, compared by identity.
        #[derive(Clone, Copy)]
        pub struct $name<'game> {
            ptr: core::ptr::NonNull<raw::$name>,
            brand: crate::session::Brand<'game>,
        }

        impl<'game> $name<'game> {
            /// # Safety
            ///
            #[doc = concat!("`ptr` must be null or point to a `", $cpp, "` of the match `brand` belongs to.")]
            #[allow(dead_code)]
            pub(crate) unsafe fn from_raw(brand: crate::session::Brand<'game>, ptr: *mut raw::$name) -> Option<Self> {
                core::ptr::NonNull::new(ptr).map(|ptr| $name { ptr, brand })
            }

            #[allow(dead_code)]
            pub(crate) fn brand(self) -> crate::session::Brand<'game> {
                self.brand
            }

            #[allow(dead_code)]
            pub(crate) fn as_ptr(self) -> *mut raw::$name {
                self.ptr.as_ptr()
            }
        }

        impl PartialEq for $name<'_> {
            fn eq(&self, other: &Self) -> bool {
                self.ptr == other.ptr
            }
        }

        impl Eq for $name<'_> {}

        impl core::hash::Hash for $name<'_> {
            fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
                self.ptr.hash(state);
            }
        }

        impl core::fmt::Debug for $name<'_> {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, concat!(stringify!($name), "({:p})"), self.ptr)
            }
        }
    };
}

/// A BWAPI `Type<T>`: an id. Every value is valid; BWAPI treats ids it does not
/// know as `Unknown`.
macro_rules! type_id {
    ($name:ident, $cpp:literal) => {
        #[doc = concat!("BWAPI: `", $cpp, "`")]
        #[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
        #[repr(transparent)]
        pub struct $name(i32);

        impl $name {
            /// Wraps a BWAPI id.
            pub const fn from_id(id: i32) -> Self {
                $name(id)
            }

            /// The BWAPI id.
            pub const fn id(self) -> i32 {
                self.0
            }
        }
    };
}

/// A C++ enum whose values the user passes in: `#[repr(i32)]`, with `TryFrom<i32>`
/// for values that come from elsewhere.
macro_rules! c_enum {
    ($name:ident, $cpp:literal, $($variant:ident = $value:literal, $variant_cpp:literal;)*) => {
        #[doc = concat!("BWAPI: `", $cpp, "`")]
        #[repr(i32)]
        #[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
        pub enum $name {
            $(
                #[doc = concat!("BWAPI: `", $cpp, "::", $variant_cpp, "`")]
                $variant = $value,
            )*
        }

        impl TryFrom<i32> for $name {
            /// The value that matches no variant.
            type Error = i32;

            fn try_from(value: i32) -> Result<Self, i32> {
                match value {
                    $($value => Ok($name::$variant),)*
                    _ => Err(value),
                }
            }
        }
    };
}

/// An opaque C++ object behind a raw pointer.
macro_rules! opaque {
    ($name:ident, $cpp:literal) => {
        #[doc = concat!("Opaque `", $cpp, "`.")]
        #[repr(C)]
        pub(crate) struct $name {
            _opaque: [u8; 0],
            _marker: core::marker::PhantomData<(*mut u8, core::marker::PhantomPinned)>,
        }
    };
}

pub(crate) use {c_enum, handle, opaque, type_id};
