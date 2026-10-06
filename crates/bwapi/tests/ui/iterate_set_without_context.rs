// The contents of a live set are read only with a `Context`.

use bwapi::UnitSet;

fn walk(set: UnitSet<'_>) {
    for _unit in set {}
}

fn main() {}
