//! The callback context: `'frame` lasts one callback, `'game` the whole match.

use super::brand::NotSendSync;
use crate::Game;

/// Lives on the stack of one callback dispatch: `'frame` borrows it.
pub(crate) struct FrameToken {
    _thread: NotSendSync,
}

impl FrameToken {
    pub(crate) const fn new() -> Self {
        FrameToken {
            _thread: NotSendSync::new(),
        }
    }
}

/// What a callback receives: the game, and the proof that the callback is running.
///
/// The contents of live sets (`UnitSet` and others) change between frames, so
/// reading them requires a `Context`. Iterators borrow `'frame` and cannot outlive
/// the callback; handles and set descriptors only carry `'game` and can be stored.
#[derive(Clone, Copy)]
pub struct Context<'frame, 'game> {
    game: Game<'game>,
    _frame: &'frame FrameToken,
}

impl<'frame, 'game> Context<'frame, 'game> {
    pub(crate) fn new(game: Game<'game>, frame: &'frame FrameToken) -> Self {
        Context {
            game,
            _frame: frame,
        }
    }

    pub fn game(self) -> Game<'game> {
        self.game
    }
}
