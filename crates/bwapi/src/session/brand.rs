//! The match brand `'game`.
//!
//! Every handle carries `Brand<'game>`. Two properties, two markers:
//! - `InvariantLifetime`: `Unit<'a>` and `Unit<'b>` never coerce into each other,
//!   so handles of different matches cannot meet;
//! - `NotSendSync`: handles stay on the thread that runs the callbacks.
//!
//! A brand is a capability: only its holder can turn a raw pointer into a handle.
//! Brands are created in one place, `lifecycle::open`, once per match; generated code
//! takes the brand of the receiver and never creates one.

use core::marker::PhantomData;

/// Makes `'game` invariant.
#[derive(Clone, Copy)]
pub(crate) struct InvariantLifetime<'game>(PhantomData<fn(&'game ()) -> &'game ()>);

/// Removes the `Send` and `Sync` auto traits.
#[derive(Clone, Copy)]
pub(crate) struct NotSendSync(PhantomData<*mut ()>);

impl NotSendSync {
    pub(crate) const fn new() -> Self {
        NotSendSync(PhantomData)
    }
}

/// The right to brand pointers of one match as `'game`.
#[derive(Clone, Copy)]
pub(crate) struct Brand<'game> {
    _game: InvariantLifetime<'game>,
    _thread: NotSendSync,
}

impl Brand<'_> {
    /// # Safety
    ///
    /// Only `lifecycle::open` may call this, once per match: every brand of a
    /// match must come from the same call.
    pub(crate) unsafe fn new() -> Self {
        Brand {
            _game: InvariantLifetime(PhantomData),
            _thread: NotSendSync::new(),
        }
    }
}
