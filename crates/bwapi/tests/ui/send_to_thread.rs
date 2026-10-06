// A handle cannot move to another thread.

use bwapi::Unit;

fn spawn(unit: Unit<'static>) {
    std::thread::spawn(move || unit.id());
}

fn main() {}
