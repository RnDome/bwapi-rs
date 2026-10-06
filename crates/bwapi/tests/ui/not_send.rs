// Handles, the game and the context are not `Send`.

use bwapi::{Context, Game, Unit};

fn assert_send<T: Send>() {}

fn main() {
    assert_send::<Unit<'static>>();
    assert_send::<Game<'static>>();
    assert_send::<Context<'static, 'static>>();
}
