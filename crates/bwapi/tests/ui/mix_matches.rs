// Handles of two matches cannot meet: `'game` is invariant.

use bwapi::Unit;

fn mix<'a, 'b>(a: Unit<'a>, b: Unit<'b>) -> Result<(), bwapi::Error> {
    a.attack_unit(b)
}

fn main() {}
