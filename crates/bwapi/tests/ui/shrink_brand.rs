// No coercion between brands, not even to a shorter lifetime.

use bwapi::Unit;

fn shrink<'a, 'b: 'a>(unit: Unit<'b>) -> Unit<'a> {
    unit
}

fn main() {}
