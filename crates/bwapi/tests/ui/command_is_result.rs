// A command returns the error of a refusal, not a bool.

use bwapi::{Unit, UnitType};

fn morph(unit: Unit<'_>, unit_type: UnitType) {
    if unit.morph(unit_type) {}
}

fn main() {}
