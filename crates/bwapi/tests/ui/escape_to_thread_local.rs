// A handle cannot outlive the match through a thread local.

use std::cell::Cell;

use bwapi::{Bot, Context, Unit};

thread_local! {
    static LEAKED: Cell<Option<Unit<'static>>> = const { Cell::new(None) };
}

struct MyBot;

impl<'game> Bot<'game> for MyBot {
    fn on_start(_cx: Context<'_, 'game>) -> Self {
        MyBot
    }

    fn on_end(self, _cx: Context<'_, 'game>, _is_winner: bool) {}

    fn on_unit_create(&mut self, _cx: Context<'_, 'game>, unit: Unit<'game>) {
        LEAKED.with(|l| l.set(Some(unit)));
    }
}

fn main() {}
