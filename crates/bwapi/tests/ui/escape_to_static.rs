// A handle cannot outlive the match through a static.

use bwapi::{Bot, Context, Unit};

static mut LEAKED: Option<Unit<'static>> = None;

struct MyBot;

impl<'game> Bot<'game> for MyBot {
    fn on_start(_cx: Context<'_, 'game>) -> Self {
        MyBot
    }

    fn on_unit_create(&mut self, _cx: Context<'_, 'game>, unit: Unit<'game>) {
        unsafe { LEAKED = Some(unit) };
    }
}

fn main() {}
