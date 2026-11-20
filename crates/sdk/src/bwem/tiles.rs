// Port of BWEM/include/tiles.h and BWEM/src/tiles.cpp of BWEM-community
// (https://github.com/N00byEdge/BWEM-community, 9a63141).
// Copyright (c) 2015, 2017, Igor Dimitrijevic. MIT/X11 License, see LICENSE-BWEM.

//! Mini tiles (8×8 pixels) and tiles (32×32 pixels).

use bwapi::TilePosition;

use super::map::MapView;
use super::{Altitude, Neutral};

/// The area id of the walkable mini tiles of a blocking neutral.
const BLOCKING_CP: i16 = i16::MIN;

/// A mini tile: walkability, altitude and area.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MiniTile {
    /// 0 for seas; != 0 for terrain and lakes (-1 not computed yet; 1 sea or
    /// lake, an intermediate value).
    altitude: Altitude,
    /// 0 unwalkable; > 0 the id of an area; < 0 walkable terrain in no area.
    area_id: i16,
}

impl Default for MiniTile {
    fn default() -> Self {
        MiniTile {
            altitude: -1,
            area_id: -1,
        }
    }
}

impl MiniTile {
    pub(crate) fn new(altitude: Altitude, area_id: i16) -> Self {
        MiniTile { altitude, area_id }
    }

    /// Walkable for BWEM: unwalkable in BWAPI makes the 8 neighbours unwalkable
    /// too, and every mini tile of a buildable tile is walkable.
    pub fn is_walkable(self) -> bool {
        self.area_id != 0
    }

    /// Distance in pixels to the center of the nearest sea mini tile.
    pub fn altitude(self) -> Altitude {
        self.altitude
    }

    /// An unwalkable mini tile of altitude 0.
    pub fn is_sea(self) -> bool {
        self.altitude == 0
    }

    /// An unwalkable mini tile of altitude > 0: part of a small unwalkable zone.
    pub fn is_lake(self) -> bool {
        self.altitude != 0 && !self.is_walkable()
    }

    /// A walkable mini tile.
    pub fn is_terrain(self) -> bool {
        self.is_walkable()
    }

    /// 0 for seas and lakes; for terrain, the id of its area (> 0) or of a zone
    /// too small for an area (<= -2).
    pub fn area_id(self) -> i16 {
        self.area_id
    }

    pub(crate) fn set_walkable(&mut self, walkable: bool) {
        self.area_id = if walkable { -1 } else { 0 };
        self.altitude = if walkable { -1 } else { 1 };
    }

    pub(crate) fn sea_or_lake(self) -> bool {
        self.altitude == 1
    }

    pub(crate) fn set_sea(&mut self) {
        self.altitude = 0;
    }

    pub(crate) fn set_lake(&mut self) {
        self.altitude = -1;
    }

    pub(crate) fn altitude_missing(self) -> bool {
        self.altitude == -1
    }

    pub(crate) fn set_altitude(&mut self, altitude: Altitude) {
        self.altitude = altitude;
    }

    pub(crate) fn area_id_missing(self) -> bool {
        self.area_id == -1
    }

    pub(crate) fn set_area_id(&mut self, id: i16) {
        self.area_id = id;
    }

    pub(crate) fn replace_area_id(&mut self, id: i16) {
        self.area_id = id;
    }

    pub(crate) fn set_blocked(&mut self) {
        self.area_id = BLOCKING_CP;
    }

    pub(crate) fn is_blocked(self) -> bool {
        self.area_id == BLOCKING_CP
    }
}

/// A tile while the analysis writes it whole; then [`TileTerrain`] and
/// [`TileState`].
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct TileData {
    /// The bottom neutral of the stack on this tile.
    pub(crate) neutral: Option<u32>,
    pub(crate) min_altitude: Altitude,
    pub(crate) area_id: i16,
    pub(crate) buildable: bool,
    pub(crate) ground_height: u8,
    pub(crate) doodad: bool,
}

/// What never changes in a tile.
#[derive(Clone, Copy, Debug)]
pub(crate) struct TileTerrain {
    pub(crate) min_altitude: Altitude,
    pub(crate) buildable: bool,
    pub(crate) ground_height: u8,
    pub(crate) doodad: bool,
}

/// What changes in a tile when neutrals are destroyed.
#[derive(Clone, Copy, Debug)]
pub(crate) struct TileState {
    /// The bottom neutral of the stack on this tile.
    pub(crate) neutral: Option<u32>,
    pub(crate) area_id: i16,
}

/// A tile of the map.
#[derive(Clone, Copy)]
pub struct Tile<'m> {
    map: MapView<'m>,
    terrain: &'m TileTerrain,
    state: &'m TileState,
}

impl<'m> Tile<'m> {
    pub(crate) fn new(map: MapView<'m>, t: TilePosition) -> Self {
        Tile {
            map,
            terrain: &map.geometry.tiles[t],
            state: &map.state.tiles[t],
        }
    }

    /// `Game::is_buildable`. Every mini tile of a buildable tile is walkable.
    pub fn is_buildable(self) -> bool {
        self.terrain.buildable
    }

    /// The area id of its walkable mini tiles if they share one, -1 if they do
    /// not, 0 if none is walkable.
    pub fn area_id(self) -> i16 {
        self.state.area_id
    }

    /// The least altitude of its mini tiles.
    pub fn min_altitude(self) -> Altitude {
        self.terrain.min_altitude
    }

    /// At least one of its mini tiles is walkable.
    pub fn is_walkable(self) -> bool {
        self.state.area_id != 0
    }

    pub fn is_terrain(self) -> bool {
        self.is_walkable()
    }

    /// 0 low ground, 1 high ground, 2 very high ground: `getGroundHeight / 2`.
    pub fn ground_height(self) -> i32 {
        i32::from(self.terrain.ground_height)
    }

    /// Part of a doodad: `getGroundHeight % 2`.
    pub fn is_doodad(self) -> bool {
        self.terrain.doodad
    }

    /// The neutral on this tile; with stacked neutrals, the bottom one.
    pub fn neutral(self) -> Option<Neutral<'m>> {
        self.state.neutral.map(|i| Neutral::new(self.map, i))
    }

    /// The number of neutrals stacked on this tile.
    pub fn stacked_neutrals(self) -> i32 {
        let mut count = 0;
        let mut next = self.state.neutral;
        while let Some(i) = next {
            count += 1;
            next = self.map.state.neutrals[i as usize].next_stacked;
        }
        count
    }
}
