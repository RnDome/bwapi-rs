//! The session: one bot per match, from `MatchStart` to `MatchEnd`.
//!
//! The C++ module of `bwapi_c_new_module` (cpp/aimodule.cpp) turns every BWAPI
//! callback into a `BWAPI::Event` and calls `bwapi_c_on_event`. Lifecycle (`Module::session` is
//! null while idle and after the end):
//!
//! ```text
//! gameInit      → BroodwarPtr = game
//! newAIModule   → Module { start, session: null }                   idle
//! MatchStart    → brand created, session = start(cx)                live
//! other events  → &mut session for one dispatch, then the borrow ends
//! MatchEnd      → session taken (null), bot ends and is dropped     ended
//! ~C++ module   → Module is leaked, a live session is never dropped
//! ```
//!
//! Safety argument:
//! 1. `Brand<'game>` is created only in `open`, once per match; a live module has
//!    at most one session.
//! 2. The session, with the bot and all of its handles, is dropped at `MatchEnd`,
//!    while the game and its objects are still alive.
//! 3. `end` nulls the session before the bot ends: no later event reaches it.
//! 4. Raw event arguments are branded only with the brand of this session.
//! 5. `&mut` to the session lives for one dispatch; a nested callback aborts.
//! 6. A panic aborts: every entry point from C++ is `extern "C"`.
//! 7. A module destroyed with a live session leaks it: the game may be gone, and
//!    the bot's `Drop` could call into BWAPI.
//! 8. `Context` is created only during a dispatch and borrows a `FrameToken` on its
//!    stack, so `'frame` cannot outlive the callback.
//!
//! `'game` disappears when the session is stored as `*mut c_void`; the session is
//! turned back into a reference only inside `Module::dispatch` and `Module::end`,
//! and no reference leaves them.

use core::cell::Cell;
use core::ffi::c_void;
use core::ptr;

use super::brand::Brand;
use super::context::{Context, FrameToken};
use crate::bot::Bot;
use crate::generated::raw;
use crate::runtime::event::Event;
use crate::{EventType, Game, Player, Unit};

/// The bot of one match and its game.
struct Session<'game, B> {
    bot: B,
    game: Game<'game>,
}

/// A session with the bot type erased: what `Module` stores.
trait Dispatch {
    /// # Safety
    ///
    /// Handles in `event` must belong to the match of this session.
    unsafe fn dispatch(&mut self, kind: EventType, event: &Event);
    fn end(self: Box<Self>, is_winner: bool);
}

impl<'game, B: Bot<'game>> Dispatch for Session<'game, B> {
    unsafe fn dispatch(&mut self, kind: EventType, event: &Event) {
        let brand = self.game.brand();
        // SAFETY (both): the caller guarantees that the handles belong to this
        // match (point 4).
        let unit =
            || unsafe { Unit::from_raw(brand, event.unit()) }.expect("BWAPI passed a null unit");
        let player = || {
            unsafe { Player::from_raw(brand, event.player()) }.expect("BWAPI passed a null player")
        };
        let text = event.text();

        let token = FrameToken::new();
        let cx = Context::new(self.game, &token);
        let bot = &mut self.bot;
        match kind {
            EventType::MatchFrame => bot.on_frame(cx),
            EventType::SendText => bot.on_send_text(cx, text),
            EventType::ReceiveText => bot.on_receive_text(cx, player(), text),
            EventType::PlayerLeft => bot.on_player_left(cx, player()),
            EventType::NukeDetect => bot.on_nuke_detect(cx, event.position()),
            EventType::UnitDiscover => bot.on_unit_discover(cx, unit()),
            EventType::UnitEvade => bot.on_unit_evade(cx, unit()),
            EventType::UnitShow => bot.on_unit_show(cx, unit()),
            EventType::UnitHide => bot.on_unit_hide(cx, unit()),
            EventType::UnitCreate => bot.on_unit_create(cx, unit()),
            EventType::UnitDestroy => bot.on_unit_destroy(cx, unit()),
            EventType::UnitMorph => bot.on_unit_morph(cx, unit()),
            EventType::UnitRenegade => bot.on_unit_renegade(cx, unit()),
            EventType::SaveGame => bot.on_save_game(cx, text),
            EventType::UnitComplete => bot.on_unit_complete(cx, unit()),
            EventType::MatchStart | EventType::MatchEnd => {
                unreachable!("the module handles {kind:?}")
            }
            EventType::MenuFrame | EventType::None => {}
        }
    }

    fn end(self: Box<Self>, is_winner: bool) {
        let token = FrameToken::new();
        let cx = Context::new(self.game, &token);
        self.bot.on_end(cx, is_winner);
    }
}

/// A session with the bot type erased. Opaque: `bot!` only passes it along.
pub struct ErasedSession<'game>(Box<dyn Dispatch + 'game>);

/// Creates the session of a match. Instantiated by `bot!` for the bot type.
pub fn start<'game, B: Bot<'game> + 'game>(cx: Context<'_, 'game>) -> ErasedSession<'game> {
    ErasedSession(Box::new(Session {
        bot: B::on_start(cx),
        game: cx.game(),
    }))
}

/// `start` for the bot type, generic over the match.
pub type StartFn = for<'frame, 'game> fn(Context<'frame, 'game>) -> ErasedSession<'game>;

/// What the C++ module holds as `void*`.
struct Module {
    start: StartFn,
    /// `Box<ErasedSession<'game>>` with `'game` erased, or null.
    session: *mut c_void,
}

impl Module {
    /// # Safety
    ///
    /// `game` must be the live game.
    unsafe fn start(&mut self, game: *mut raw::Game) {
        if !self.session.is_null() {
            abort("BWAPI started a match twice");
        }
        let session = unsafe { open(self.start, game) };
        self.session = Box::into_raw(Box::new(session)).cast();
    }

    /// Any event between start and end. Ignored without a live session.
    ///
    /// # Safety
    ///
    /// Handles in `event` must belong to the running match.
    unsafe fn dispatch(&mut self, kind: EventType, event: &Event) {
        if self.session.is_null() {
            return;
        }
        // SAFETY: a live session; the guard makes this the only reference (point 5).
        let session = unsafe { &mut *self.session.cast::<ErasedSession<'_>>() };
        unsafe { session.0.dispatch(kind, event) };
    }

    /// Detaches the session first, then ends the bot.
    fn end(&mut self, is_winner: bool) {
        let session = core::mem::replace(&mut self.session, ptr::null_mut());
        if session.is_null() {
            return;
        }
        // SAFETY: the session is detached (point 3) and owned here.
        let session = unsafe { Box::from_raw(session.cast::<ErasedSession<'_>>()) };
        session.0.end(is_winner);
    }
}

/// The only place a brand is created.
///
/// # Safety
///
/// `game` must be the live game; called once per match.
unsafe fn open<'game>(start: StartFn, game: *mut raw::Game) -> ErasedSession<'game> {
    // SAFETY: one brand per match, created here only (point 1).
    let brand = unsafe { Brand::<'game>::new() };
    let game = unsafe { Game::from_raw(brand, game) }.expect("BWAPI passed a null game");
    let token = FrameToken::new();
    start(Context::new(game, &token))
}

unsafe extern "C" {
    fn bwapi_c_game_init(game: *mut raw::Game);
    fn bwapi_c_new_module(module: *mut c_void) -> *mut c_void;
}

/// # Safety
///
/// `game` must be the `BWAPI::Game*` the host passes to `gameInit`.
pub unsafe fn game_init(game: *mut c_void) {
    unsafe { bwapi_c_game_init(game.cast()) }
}

/// Returns the `BWAPI::AIModule*` that `newAIModule` hands to the host.
pub fn new_module(start: StartFn) -> *mut c_void {
    let module = Box::into_raw(Box::new(Module {
        start,
        session: ptr::null_mut(),
    }));
    // SAFETY: the C++ module takes the module and passes it back with every event.
    unsafe { bwapi_c_new_module(module.cast()) }
}

/// Every BWAPI callback. The C++ module passes the module it was created with, the
/// live game and an event valid for the duration of the call.
#[unsafe(no_mangle)]
extern "C" fn bwapi_c_on_event(module: *mut c_void, game: *mut raw::Game, event: *const Event) {
    let _guard = ReentrancyGuard::enter();
    // SAFETY: see above; the guard makes `module` the only reference (point 5).
    let (module, event) = unsafe { (&mut *module.cast::<Module>(), &*event) };
    let Ok(kind) = EventType::try_from(event.event_type()) else {
        abort("BWAPI passed an unknown event type");
    };
    match kind {
        // SAFETY: `game` is the live game.
        EventType::MatchStart => unsafe { module.start(game) },
        EventType::MatchEnd => module.end(event.is_winner()),
        // SAFETY: BWAPI passes objects of the running match.
        _ => unsafe { module.dispatch(kind, event) },
    }
}

thread_local! {
    static IN_CALLBACK: Cell<bool> = const { Cell::new(false) };
}

/// Aborts on a nested callback: two `&mut` to the session must never exist.
struct ReentrancyGuard;

impl ReentrancyGuard {
    fn enter() -> Self {
        if IN_CALLBACK.with(|c| c.replace(true)) {
            abort("BWAPI called the bot from inside a callback");
        }
        ReentrancyGuard
    }
}

impl Drop for ReentrancyGuard {
    fn drop(&mut self) {
        IN_CALLBACK.with(|c| c.set(false));
    }
}

fn abort(msg: &str) -> ! {
    eprintln!("bwapi-rs: {msg}");
    std::process::abort();
}
