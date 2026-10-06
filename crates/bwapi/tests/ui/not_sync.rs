// Handles, the game and the context are not `Sync`.

use bwapi::{Context, Game, Unit};

fn assert_sync<T: Sync>() {}

fn main() {
    assert_sync::<Unit<'static>>();
    assert_sync::<Game<'static>>();
    assert_sync::<Context<'static, 'static>>();
}
