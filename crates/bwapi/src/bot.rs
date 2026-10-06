//! The bot: what the user implements and exports.

use crate::{BwStr, Context, Player, Position, Unit};

/// A bot. One bot lives for one match `'game`: `on_start` creates it, it receives
/// every other callback, and `on_end` consumes it.
///
/// Every callback receives a `Context`. Handles (`Unit<'game>` and others) can be
/// stored in the bot for the whole match; iterators over live sets cannot.
#[allow(unused_variables)]
pub trait Bot<'game>: Sized {
    /// BWAPI: `AIModule::onStart`. The match starts: create the bot.
    fn on_start(cx: Context<'_, 'game>) -> Self;
    /// BWAPI: `AIModule::onEnd`. The match is over: the bot is dropped after this call.
    fn on_end(self, cx: Context<'_, 'game>, is_winner: bool) {}
    /// BWAPI: `AIModule::onFrame`
    fn on_frame(&mut self, cx: Context<'_, 'game>) {}
    /// BWAPI: `AIModule::onSendText`
    fn on_send_text(&mut self, cx: Context<'_, 'game>, text: &BwStr) {}
    /// BWAPI: `AIModule::onReceiveText`
    fn on_receive_text(&mut self, cx: Context<'_, 'game>, player: Player<'game>, text: &BwStr) {}
    /// BWAPI: `AIModule::onPlayerLeft`
    fn on_player_left(&mut self, cx: Context<'_, 'game>, player: Player<'game>) {}
    /// BWAPI: `AIModule::onNukeDetect`
    fn on_nuke_detect(&mut self, cx: Context<'_, 'game>, target: Position) {}
    /// BWAPI: `AIModule::onUnitDiscover`
    fn on_unit_discover(&mut self, cx: Context<'_, 'game>, unit: Unit<'game>) {}
    /// BWAPI: `AIModule::onUnitEvade`
    fn on_unit_evade(&mut self, cx: Context<'_, 'game>, unit: Unit<'game>) {}
    /// BWAPI: `AIModule::onUnitShow`
    fn on_unit_show(&mut self, cx: Context<'_, 'game>, unit: Unit<'game>) {}
    /// BWAPI: `AIModule::onUnitHide`
    fn on_unit_hide(&mut self, cx: Context<'_, 'game>, unit: Unit<'game>) {}
    /// BWAPI: `AIModule::onUnitCreate`
    fn on_unit_create(&mut self, cx: Context<'_, 'game>, unit: Unit<'game>) {}
    /// BWAPI: `AIModule::onUnitDestroy`
    fn on_unit_destroy(&mut self, cx: Context<'_, 'game>, unit: Unit<'game>) {}
    /// BWAPI: `AIModule::onUnitMorph`
    fn on_unit_morph(&mut self, cx: Context<'_, 'game>, unit: Unit<'game>) {}
    /// BWAPI: `AIModule::onUnitRenegade`
    fn on_unit_renegade(&mut self, cx: Context<'_, 'game>, unit: Unit<'game>) {}
    /// BWAPI: `AIModule::onSaveGame`
    fn on_save_game(&mut self, cx: Context<'_, 'game>, game_name: &BwStr) {}
    /// BWAPI: `AIModule::onUnitComplete`
    fn on_unit_complete(&mut self, cx: Context<'_, 'game>, unit: Unit<'game>) {}
}

/// Exports the entry points the BWAPI host looks for: `gameInit` and `newAIModule`.
///
/// The bot type may have a `'game` lifetime or none:
///
/// ```ignore
/// struct MyBot<'game> { scouts: Vec<bwapi::Unit<'game>> }
/// impl<'game> bwapi::Bot<'game> for MyBot<'game> {
///     fn on_start(cx: bwapi::Context<'_, 'game>) -> Self { MyBot { scouts: Vec::new() } }
/// }
/// bwapi::bot!(MyBot);
/// ```
#[macro_export]
macro_rules! bot {
    ($bot:ty) => {
        #[unsafe(no_mangle)]
        #[allow(non_snake_case)]
        extern "C" fn gameInit(game: *mut ::core::ffi::c_void) {
            // SAFETY: the host passes its game before it creates the module.
            unsafe { $crate::__rt::game_init(game) }
        }

        #[unsafe(no_mangle)]
        #[allow(non_snake_case)]
        extern "C" fn newAIModule() -> *mut ::core::ffi::c_void {
            // Generic over `'game`: the bot is checked for an arbitrary match.
            fn start<'frame, 'game>(
                cx: $crate::Context<'frame, 'game>,
            ) -> $crate::__rt::ErasedSession<'game> {
                $crate::__rt::start::<$bot>(cx)
            }
            $crate::__rt::new_module(start)
        }
    };
}
