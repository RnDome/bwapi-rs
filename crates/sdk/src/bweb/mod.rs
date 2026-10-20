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
