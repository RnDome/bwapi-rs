// `'frame` cannot be extended to `'game`.

use bwapi::{Context, UnitSet, UnitSetIter};

fn leak<'frame, 'game>(cx: Context<'frame, 'game>, set: UnitSet<'game>) -> UnitSetIter<'game, 'game> {
    set.iter(cx)
}

fn main() {}
