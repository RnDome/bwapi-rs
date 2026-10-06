//! `BWAPI::Event`, the Rust side of cpp/event.cpp: one BWAPI callback, read
//! through its own methods. Which fields are set depends on the type.

use core::ptr;

use super::ffi::Pos;
use crate::generated::raw;
use crate::{BwStr, Position};

super::types::opaque!(Event, "BWAPI::Event");

// The `Event_*` functions of cpp/include/bwapi_c.h.
#[allow(non_snake_case)]
unsafe extern "C" {
    fn Event_getType(event: *const Event) -> i32;
    fn Event_getUnit(event: *const Event) -> *mut raw::Unit;
    fn Event_getPlayer(event: *const Event) -> *mut raw::Player;
    fn Event_getPosition(event: *const Event) -> Pos;
    fn Event_getText(event: *const Event, ptr: *mut *const u8, len: *mut usize);
    fn Event_isWinner(event: *const Event) -> bool;
}

// SAFETY (every method): `&Event` exists only for an event that the C++ module
// passed to `bwapi_c_on_event`, alive for the whole callback.
impl Event {
    /// `BWAPI::EventType::Enum`.
    pub(crate) fn event_type(&self) -> i32 {
        unsafe { Event_getType(self) }
    }

    pub(crate) fn unit(&self) -> *mut raw::Unit {
        unsafe { Event_getUnit(self) }
    }

    pub(crate) fn player(&self) -> *mut raw::Player {
        unsafe { Event_getPlayer(self) }
    }

    pub(crate) fn position(&self) -> Position {
        unsafe { Event_getPosition(self) }.into()
    }

    /// The text belongs to the event.
    pub(crate) fn text(&self) -> &BwStr {
        let (mut text, mut len) = (ptr::null(), 0);
        unsafe { Event_getText(self, &mut text, &mut len) };
        // SAFETY: the bytes of a `std::string` owned by the event.
        BwStr::from_bytes(unsafe { core::slice::from_raw_parts(text, len) })
    }

    pub(crate) fn is_winner(&self) -> bool {
        unsafe { Event_isWinner(self) }
    }
}
