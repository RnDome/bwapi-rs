// The context of a callback cannot be kept past it.

use bwapi::{Bot, Context};

struct MyBot<'game> {
    cx: Option<Context<'game, 'game>>,
}

impl<'game> Bot<'game> for MyBot<'game> {
    fn on_start(_cx: Context<'_, 'game>) -> Self {
        MyBot { cx: None }
    }

    fn on_frame(&mut self, cx: Context<'_, 'game>) {
        self.cx = Some(cx);
    }
}

fn main() {}
