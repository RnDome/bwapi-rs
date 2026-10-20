// Port of BWEM-community (https://github.com/N00byEdge/BWEM-community, 9a63141).
// Copyright (c) 2015, 2017, Igor Dimitrijevic. MIT/X11 License, see LICENSE-BWEM.

//! BWEM, the Brood War Easy Map: an unofficial port of the map analyser by Igor
//! Dimitrijevic.
//!
//! [`Map::initialize`] analyses a map once: altitudes, seas and lakes, blocking
//! neutrals, areas, choke points, the distances between choke points and the
//! bases. The map keeps what never changes after the analysis ([`Geometry`])
//! apart from what changes when neutrals are destroyed ([`State`]). Ids
//! ([`AreaId`], [`ChokePointId`], [`BaseId`], [`NeutralId`]) name objects and
//! borrow nothing; views ([`Area`], [`ChokePoint`], [`Base`], [`Neutral`],
//! [`Tile`]) borrow the map and read both parts as they are now.
//!
//! Differences from the C++ code:
//! - the input is data, [`MapInput`], not a `BWAPI::Game`;
//! - the map is a value, not the singleton `Map::Instance()`;
//! - the scratch data of the algorithms (`Markable`, `Tile::InternalData`,
//!   the `static` counter of `chooseNeighboringArea`) is local to an analysis,
//!   so two analyses of one map give one result;
//! - lookups return `Option` where C++ asserts or returns -1;
//! - a neutral overlapping a stack without being aligned with it is removed
//!   without following the stack past its end (C++ dereferences null there,
//!   on Sparkle 1.1);
//! - `ChokePoint::width` is added: BWEB calls it from a fork of BWEM.
//!
//! The results are those of BWEM-community built with GCC: where it sorts with
//! `std::sort`, the port sorts as libstdc++ does, since the order of equal
//! altitudes changes altitudes and areas. On every map of the tests of
//! BWEM-community the port gives the same altitudes, areas, choke points,
//! distances and bases as the C++ code on OpenBW. BWEM built with MSVC sorts
//! otherwise and may give other areas.
//!
//! Kept as in C++, though they look like mistakes:
//! - a mineral or geyser goes to the area with the greatest id among the areas
//!   of its tiles (`mainArea` takes the last key of its map), not to the one with
//!   most of its tiles;
//! - a base gets the blocking minerals of the last location checked, which may
//!   be another location than the base's.

mod area;
mod base;
mod cp;
mod geometry_helpers;
mod graph;
mod map;
mod neutral;
mod tiles;

pub use area::{Area, GroundHeights};
pub use base::Base;
pub use cp::{ChokePoint, ChokePointPath, Node};
pub use map::{CellPosition, Changes, Geometry, Map, MapPosition, State, TileRect};
pub use neutral::Neutral;
pub use tiles::{MiniTile, Tile};

use alloc::vec::Vec;
use core::fmt;

use bwapi::{Position, TilePosition, WalkPosition};

use crate::grid::Grid;

/// Altitudes, in pixels: `altitude_t`.
pub type Altitude = i16;

/// The constants of BWEM's `defs.h`.
pub mod detail {
    /// A zone of unwalkable mini tiles is a lake if it has at most this many
    /// mini tiles…
    pub const LAKE_MAX_MINI_TILES: usize = 300;
    /// …and spans at most this many mini tiles in each direction.
    pub const LAKE_MAX_WIDTH_IN_MINI_TILES: i32 = 8 * 4;
    /// Connected mini tiles needed for an area.
    pub const AREA_MIN_MINI_TILES: i32 = 64;
    pub const MAX_TILES_BETWEEN_COMMAND_CENTER_AND_RESOURCES: i32 = 10;
    pub const MIN_TILES_BETWEEN_BASES: i32 = 10;
    pub const MAX_TILES_BETWEEN_STARTING_LOCATION_AND_ITS_ASSIGNED_BASE: i32 = 3;
}

/// The id of an area: 1 and up. Mini tiles and tiles also carry ids of no area:
/// see [`MiniTile::area_id`] and [`Tile::area_id`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AreaId(i16);

impl AreaId {
    /// The area id `id`, if it is one (> 0).
    pub fn new(id: i16) -> Option<Self> {
        (id > 0).then_some(AreaId(id))
    }

    pub fn get(self) -> i16 {
        self.0
    }
}

/// The id of a choke point: its index in the distance matrix.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ChokePointId(pub(crate) u32);

impl ChokePointId {
    /// `ChokePoint::Index()`: its index in the distance matrix, 0 and up.
    pub fn index(self) -> u32 {
        self.0
    }
}

/// The id of a base.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BaseId(pub(crate) u32);

/// The id of a neutral. A destroyed neutral keeps its id and its view, which
/// tells it is no longer alive.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NeutralId(pub(crate) u32);

/// What a static neutral is to BWEM.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NeutralKind {
    /// A mineral field.
    Mineral,
    /// `Resource_Vespene_Geyser`.
    Geyser,
    /// Any other neutral BWEM keeps: special buildings and pit doors.
    StaticBuilding,
    /// A neutral BWEM leaves out: eggs, critters, power-ups. Only BWEB looks at
    /// it.
    Other,
}

/// A static neutral unit, classified as BWEM's `InitializeNeutrals` does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NeutralInput {
    /// `Unit::id()`; BWEM does not interpret it.
    pub unit_id: i32,
    /// `UnitType::id()`; BWEM does not interpret it.
    pub type_id: i32,
    pub kind: NeutralKind,
    /// `initial_tile_position`, one tile further right for
    /// `Special_Right_Pit_Door`.
    pub top_left: TilePosition,
    /// The tile size of the initial type.
    pub size: TilePosition,
    /// `initial_position`.
    pub pos: Position,
    /// `initial_resources`.
    pub initial_amount: i32,
    /// `initial_tile_position` as BWAPI gives it, for BWEB.
    pub tile_position: TilePosition,
    /// `UnitType::is_building`: BWEB marks the tiles as used.
    pub building: bool,
    /// `UnitType::top_speed` is 0: BWEB reserves the tiles.
    pub immobile: bool,
}

/// What BWEM and BWEB read from the game: walkability of every mini tile,
/// buildability and ground height of every tile, the static neutrals and the
/// start locations. Neutrals in the order `getStaticNeutralUnits` gives them:
/// the order decides how stacked neutrals are linked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapInput {
    /// `Game::is_walkable` of every mini tile. BWEM erodes it on analysis; BWEB
    /// reads it as it is.
    pub walkable: Grid<WalkPosition, bool>,
    /// `Game::is_buildable` of every tile.
    pub buildable: Grid<TilePosition, bool>,
    /// `Game::ground_height` of every tile: 0..=5, level * 2 + doodad.
    pub ground_height: Grid<TilePosition, u8>,
    pub neutrals: Vec<NeutralInput>,
    pub start_locations: Vec<TilePosition>,
}

/// An inconsistent [`MapInput`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InputError {
    /// The map has no tiles.
    EmptyMap,
    /// A grid does not match the size of `buildable`.
    GridSize {
        grid: &'static str,
        width: i32,
        height: i32,
    },
    /// A ground height above 5.
    GroundHeight { tile: TilePosition, height: u8 },
    /// A neutral with no tiles or with tiles outside the map.
    NeutralOutside { unit_id: i32 },
}

impl fmt::Display for InputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InputError::EmptyMap => f.write_str("the map has no tiles"),
            InputError::GridSize {
                grid,
                width,
                height,
            } => write!(
                f,
                "the {grid} grid is {width}x{height}, which does not fit the map"
            ),
            InputError::GroundHeight { tile, height } => {
                write!(f, "ground height {height} at {tile:?} is above 5")
            }
            InputError::NeutralOutside { unit_id } => {
                write!(f, "neutral {unit_id} is not inside the map")
            }
        }
    }
}

impl core::error::Error for InputError {}
