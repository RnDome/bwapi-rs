// Port of BWEB (https://github.com/Cmccrave/BWEB, v1.17, d57980c).
// Copyright (c) 2017 Christian McCrave. MIT License, see LICENSE-BWEB.

//! BWEB, the Brood War Easy Builder: an unofficial port of the building
//! placement library by Christian McCrave.
//!
//! [`Layout`] keeps the tiles used by buildings, the tiles reserved for bases
//! and mining, and the walkability of tiles by unit size; it checks placement
//! and estimates ground distances through the choke points of [`bwem`]. It is
//! what BWEB keeps in the namespace `BWEB::Map`; the port does not call it a map
//! next to [`bwem::Map`]. [`Stations`] finds the main and the natural of every
//! start location and their choke points, and the race presets around each
//! base. [`Blocks`] lays out the building slots of the race in the mains;
//! [`Walls`] closes choke points with buildings, defenses and openings.
//! [`Path`] searches tiles with A* or with the Jump Point Search of
//! False.Genesis (public domain), which BWEB carries in `JPS.h`;
//! [`Pathfinding`] holds what BWEB keeps between searches, the cache of
//! unreachable tiles among it.
//!
//! Differences from the C++ code:
//! - the input is data: [`MapInput`] and the analysed [`bwem::Map`], passed to
//!   the calls that need it, not the globals `Broodwar` and `BWEM::Map::Instance()`;
//! - a unit type is a [`Footprint`], what BWEB reads of a `BWAPI::UnitType`;
//! - [`Layout::can_place`] checks a resource depot against the minerals and
//!   geysers of the map, as `canBuildHere` does, but not against units or creep;
//!   those need the game;
//! - [`Layout::ground_distance_via_choke_points`] gives `None` where BWEB gives
//!   `DBL_MAX`, and also where BWEB gives the straight distance between two
//!   positions with no path between them;
//! - `findStations` is two steps: [`Stations::analyze`] finds the stations,
//!   taking the start location and the race of the bot as arguments instead of
//!   `Broodwar->self()`, and [`Stations::apply_reserves`] reserves their tiles;
//! - the angles of station defenses come from `atan2` of musl, which may differ
//!   from that of the C library of C++ in the last bit;
//! - the race presets name their unit types; [`Footprint::of`] gives those
//!   without the game;
//! - what blocks and walls ask the game, creep and `canBuildHere`, comes from a
//!   [`StartState`]; the grid of block tiles BWEB fills and never reads is not
//!   kept, nor is the log of walls BWEB writes, `BWEB_Wall.txt`;
//! - where BWEB dereferences a missing station, base or choke point, the port
//!   makes no start blocks or no wall;
//! - `ChokePoint::Width()`, which BWEB calls, is the one of McRave's fork of
//!   BWEM: [`bwem::ChokePoint::width`];
//! - where neutral buildings of several types overlap, [`Layout::is_used`] gives
//!   the type of the last in the order of [`MapInput`]; BWEB gives the last in
//!   the order of `getNeutralUnits`, an `unordered_set`.
//!
//! But for these, on every map of the tests of BWEM-community the port gives the
//! same walkability, used and reserved tiles, placements, ground distances,
//! stations, walls, blocks and paths as BWEB v1.17 on BWEM-community on OpenBW,
//! for each race. The open lists
//! of A* and JPS are heaps; the port orders them as libstdc++ does, since the
//! order of equal costs picks the path.
//!
//! [`bwem`]: crate::bwem
//! [`bwem::Map`]: crate::bwem::Map
//! [`bwem::ChokePoint::width`]: crate::bwem::ChokePoint::width
//! [`MapInput`]: crate::bwem::MapInput

mod block;
mod jps;
mod layout;
mod path;
mod station;
mod wall;

pub use block::{Block, BlockType, Blocks, Piece, StartState};
pub use layout::{Layout, perpendicular_line};
pub use path::{Path, Pathfinding};
pub use station::{Station, Stations};
pub use wall::{Wall, Walls};

use bwapi::UnitType;

/// What BWEB reads of a unit type. `adapter::footprint` makes it from a
/// `UnitType`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Footprint {
    /// `UnitType::id()`, what [`Layout::is_used`] reports.
    pub type_id: i32,
    /// The size in tiles.
    pub tile_width: i32,
    pub tile_height: i32,
    /// The size in pixels, which picks the walkability of [`Layout::is_walkable`].
    pub width: i32,
    pub height: i32,
    pub requires_creep: bool,
    pub is_resource_depot: bool,
}

impl Footprint {
    /// `UnitTypes::None`: no size, so walkability of whole tiles.
    pub const NONE: Footprint = Footprint {
        type_id: UnitType::NONE.id(),
        tile_width: 0,
        tile_height: 0,
        width: 0,
        height: 0,
        requires_creep: false,
        is_resource_depot: false,
    };

    /// The footprint of `ty` if it is a type the race presets of BWEB place,
    /// without the game: those presets name their types.
    pub fn of(ty: UnitType) -> Option<Footprint> {
        KNOWN.iter().find(|f| f.type_id == ty.id()).copied()
    }
}

/// A building of `ty`: tiles, pixels, on creep, a resource depot.
const fn building(
    ty: UnitType,
    tiles: (i32, i32),
    pixels: (i32, i32),
    creep: bool,
    depot: bool,
) -> Footprint {
    Footprint {
        type_id: ty.id(),
        tile_width: tiles.0,
        tile_height: tiles.1,
        width: pixels.0,
        height: pixels.1,
        requires_creep: creep,
        is_resource_depot: depot,
    }
}

/// The types the race presets of BWEB place, as BWAPI gives them.
const KNOWN: [Footprint; 16] = [
    building(
        UnitType::TERRAN_COMMAND_CENTER,
        (4, 3),
        (117, 83),
        false,
        true,
    ),
    building(
        UnitType::TERRAN_SUPPLY_DEPOT,
        (3, 2),
        (77, 49),
        false,
        false,
    ),
    building(UnitType::TERRAN_BARRACKS, (4, 3), (105, 73), false, false),
    building(
        UnitType::TERRAN_MISSILE_TURRET,
        (2, 2),
        (33, 49),
        false,
        false,
    ),
    building(UnitType::ZERG_HATCHERY, (4, 3), (99, 65), false, true),
    building(UnitType::ZERG_CREEP_COLONY, (2, 2), (48, 48), true, false),
    building(UnitType::ZERG_SUNKEN_COLONY, (2, 2), (48, 48), true, false),
    building(UnitType::ZERG_SPAWNING_POOL, (3, 2), (77, 47), true, false),
    building(
        UnitType::ZERG_EVOLUTION_CHAMBER,
        (3, 2),
        (77, 53),
        true,
        false,
    ),
    building(UnitType::ZERG_SPIRE, (2, 2), (57, 57), true, false),
    building(UnitType::PROTOSS_NEXUS, (4, 3), (113, 79), false, true),
    building(UnitType::PROTOSS_PYLON, (2, 2), (33, 33), false, false),
    building(UnitType::PROTOSS_GATEWAY, (4, 3), (97, 73), false, false),
    building(UnitType::PROTOSS_FORGE, (3, 2), (73, 45), false, false),
    building(
        UnitType::PROTOSS_PHOTON_CANNON,
        (2, 2),
        (41, 33),
        false,
        false,
    ),
    building(UnitType::PROTOSS_DRAGOON, (1, 1), (32, 32), false, false),
];
