//! The tests of FAP (Tests/Combat.cpp, Tests/NoLockup.cpp) on the port, with
//! unit data from BWAPILIB.

use bwapi::UnitType;
use bwapi_sdk::fap::{Options, Side, Simulator, Unit};

mod baseline;
mod combat;
mod no_lockup;
mod outcome;
mod test_unit;

use test_unit::{Upgrades, test_unit, test_unit_with};

/// Room for every unit these tests add.
const CAPACITY: usize = 16;

/// Frames for a fight to its end, where C++ calls `simulate(-1)`: its counter
/// never reaches zero, so the fight runs until it ends by itself. `u32::MAX`
/// does the same for a fight that ends sooner, as every fight here does.
const TO_THE_END: u32 = u32::MAX;

fn simulator() -> Simulator {
    Simulator::with_capacity(CAPACITY)
}

/// `setPosition`.
fn at(mut unit: Unit, x: i32, y: i32) -> Unit {
    unit.x = x;
    unit.y = y;
    unit
}
