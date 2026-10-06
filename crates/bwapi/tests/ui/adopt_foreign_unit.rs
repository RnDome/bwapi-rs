// A bot of one match cannot store a handle of another.

use bwapi::Unit;

fn adopt<'a, 'b>(units: &mut Vec<Unit<'a>>, unit: Unit<'b>) {
    units.push(unit);
}

fn main() {}
