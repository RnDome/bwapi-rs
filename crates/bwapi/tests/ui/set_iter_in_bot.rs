// An iterator over a live set cannot be kept past the callback.

use bwapi::{Bot, Context, UnitSet, UnitSetIter};

struct MyBot<'game> {
    set: UnitSet<'game>,
    iter: Option<UnitSetIter<'game, 'game>>,
}

impl<'game> Bot<'game> for MyBot<'game> {
    fn on_start(_cx: Context<'_, 'game>) -> Self {
        unimplemented!()
    }

    fn on_frame(&mut self, cx: Context<'_, 'game>) {
        self.iter = Some(self.set.iter(cx));
    }
}

fn main() {}
