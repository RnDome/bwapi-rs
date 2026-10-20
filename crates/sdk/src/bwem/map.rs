// Port of BWEM/include/map.h, BWEM/include/mapImpl.h, BWEM/src/map.cpp and
// BWEM/src/mapImpl.cpp of BWEM-community
// (https://github.com/N00byEdge/BWEM-community, 9a63141).
// Copyright (c) 2015, 2017, Igor Dimitrijevic. MIT/X11 License, see LICENSE-BWEM.

//! The map: its analysis, its queries and its updates.

use alloc::collections::{BTreeMap, VecDeque};
use alloc::vec;
use alloc::vec::Vec;

use bwapi::{Position, TilePosition, WalkPosition};

use super::area::AreaData;
use super::base::BaseData;
use super::cp::ChokePointData;
use super::ext::{dist, norm, outer_mini_tile_border};
use super::neutral::NeutralData;
use super::tiles::TileData;
use super::{
    Altitude, Area, AreaId, Base, BaseId, ChokePoint, ChokePointId, CpPath, Geyser, InputError,
    MapInput, Mineral, MiniTile, Neutral, NeutralId, NeutralKind, StaticBuilding, Tile, detail,
};
use crate::grid::Grid;
use crate::stdsort;

const DIRS4: [(i32, i32); 4] = [(0, -1), (-1, 0), (1, 0), (0, 1)];
const DIRS8: [(i32, i32); 8] = [
    (-1, -1),
    (0, -1),
    (1, -1),
    (-1, 0),
    (1, 0),
    (-1, 1),
    (0, 1),
    (1, 1),
];

/// The analysed map: `BWEM::Map`.
#[derive(Clone, Debug)]
pub struct Map {
    pub(crate) size: TilePosition,
    pub(crate) walk_size: WalkPosition,
    pub(crate) center: Position,
    pub(crate) tiles: Grid<TilePosition, TileData>,
    pub(crate) mini_tiles: Grid<WalkPosition, MiniTile>,
    pub(crate) max_altitude: Altitude,
    pub(crate) automatic_path_update: bool,

    /// Every neutral ever created; a destroyed one stays, not alive.
    pub(crate) neutrals: Vec<NeutralData>,
    pub(crate) minerals: Vec<u32>,
    pub(crate) geysers: Vec<u32>,
    pub(crate) static_buildings: Vec<u32>,
    pub(crate) starting_locations: Vec<TilePosition>,
    pub(crate) raw_frontier: Vec<((i16, i16), WalkPosition)>,

    /// Area `id` at `id - 1`.
    pub(crate) areas: Vec<AreaData>,
    /// By index.
    pub(crate) choke_points: Vec<ChokePointData>,
    /// `Graph::ChokePoints()`: in the order of the pairs of areas.
    pub(crate) choke_point_list: Vec<u32>,
    /// The choke points of each pair of areas, see [`pair_index`].
    pub(crate) cp_matrix: Vec<Vec<u32>>,
    /// `choke_points.len()²` distances, -1 when unreachable.
    pub(crate) cp_distances: Vec<i32>,
    pub(crate) cp_paths: Vec<Vec<u32>>,
    /// `ChokePoint::PathBackTrace`, kept between computations as in C++.
    pub(crate) path_back_trace: Vec<Option<u32>>,
    pub(crate) bases: Vec<BaseData>,
    pub(crate) base_count: usize,
}

/// The index in `Map::cp_matrix` of the pair of areas `a`, `b` (`a != b`, both
/// valid ids).
pub(crate) fn pair_index(a: i16, b: i16) -> usize {
    let (lo, hi) = if a < b { (a, b) } else { (b, a) };
    let (lo, hi) = (lo as usize, hi as usize);
    (hi - 1) * (hi - 2) / 2 + (lo - 1)
}

/// The scratch data of an analysis: `Tile::InternalData` and the marks of
/// `Markable`, which BWEM keeps in its objects.
pub(crate) struct Scratch {
    pub(crate) internal: Grid<TilePosition, i32>,
    /// The insertion order of a tile in a priority queue: entries of equal
    /// distance leave a `std::multimap` in insertion order.
    pub(crate) seq: Grid<TilePosition, u64>,
    mark: Grid<TilePosition, u32>,
    current_mark: u32,
}

impl Scratch {
    pub(crate) fn new(size: TilePosition) -> Self {
        Scratch {
            internal: Grid::filled(size.x, size.y, 0),
            seq: Grid::filled(size.x, size.y, 0),
            mark: Grid::filled(size.x, size.y, 0),
            current_mark: 0,
        }
    }

    pub(crate) fn unmark_all(&mut self) {
        self.current_mark += 1;
    }

    pub(crate) fn marked(&self, t: TilePosition) -> bool {
        self.mark[t] == self.current_mark
    }

    pub(crate) fn set_marked(&mut self, t: TilePosition) {
        self.mark[t] = self.current_mark;
    }
}

/// A position of any scale: `Valid` and `Crop` take each of them.
pub trait MapPosition: Copy {
    fn valid_in(self, map: &Map) -> bool;
    fn crop_in(self, map: &Map) -> Self;
}

fn crop_xy(x: i32, y: i32, size_x: i32, size_y: i32) -> (i32, i32) {
    let cx = if x < 0 {
        0
    } else if x >= size_x {
        size_x - 1
    } else {
        x
    };
    let cy = if y < 0 {
        0
    } else if y >= size_y {
        size_y - 1
    } else {
        y
    };
    (cx, cy)
}

impl MapPosition for TilePosition {
    fn valid_in(self, map: &Map) -> bool {
        (0..map.size.x).contains(&self.x) && (0..map.size.y).contains(&self.y)
    }

    fn crop_in(self, map: &Map) -> Self {
        let (x, y) = crop_xy(self.x, self.y, map.size.x, map.size.y);
        TilePosition::new(x, y)
    }
}

impl MapPosition for WalkPosition {
    fn valid_in(self, map: &Map) -> bool {
        (0..map.walk_size.x).contains(&self.x) && (0..map.walk_size.y).contains(&self.y)
    }

    fn crop_in(self, map: &Map) -> Self {
        let (x, y) = crop_xy(self.x, self.y, map.walk_size.x, map.walk_size.y);
        WalkPosition::new(x, y)
    }
}

impl MapPosition for Position {
    fn valid_in(self, map: &Map) -> bool {
        WalkPosition::from(self).valid_in(map)
    }

    fn crop_in(self, map: &Map) -> Self {
        let (x, y) = crop_xy(self.x, self.y, 32 * map.size.x, 32 * map.size.y);
        Position::new(x, y)
    }
}

/// A tile or a mini tile position: what a breadth first search and a nearest
/// area search walk on.
pub trait CellPosition: MapPosition + PartialEq {
    /// [`Tile`] for tile positions, [`MiniTile`] for walk positions.
    type Cell<'m>;

    #[doc(hidden)]
    fn cell(map: &Map, p: Self) -> Self::Cell<'_>;
    #[doc(hidden)]
    fn offset(self, dx: i32, dy: i32) -> Self;
    #[doc(hidden)]
    fn grid_index(map: &Map, p: Self) -> usize;
    #[doc(hidden)]
    fn grid_len(map: &Map) -> usize;
    #[doc(hidden)]
    fn area_id(map: &Map, p: Self) -> i16;
}

impl CellPosition for TilePosition {
    type Cell<'m> = Tile<'m>;

    fn cell(map: &Map, p: Self) -> Tile<'_> {
        Tile {
            map,
            data: &map.tiles[p],
        }
    }

    fn offset(self, dx: i32, dy: i32) -> Self {
        self + TilePosition::new(dx, dy)
    }

    fn grid_index(map: &Map, p: Self) -> usize {
        (p.y * map.size.x + p.x) as usize
    }

    fn grid_len(map: &Map) -> usize {
        (map.size.x * map.size.y) as usize
    }

    fn area_id(map: &Map, p: Self) -> i16 {
        map.tiles[p].area_id
    }
}

impl CellPosition for WalkPosition {
    type Cell<'m> = MiniTile;

    fn cell(map: &Map, p: Self) -> MiniTile {
        map.mini_tiles[p]
    }

    fn offset(self, dx: i32, dy: i32) -> Self {
        self + WalkPosition::new(dx, dy)
    }

    fn grid_index(map: &Map, p: Self) -> usize {
        (p.y * map.walk_size.x + p.x) as usize
    }

    fn grid_len(map: &Map) -> usize {
        (map.walk_size.x * map.walk_size.y) as usize
    }

    fn area_id(map: &Map, p: Self) -> i16 {
        map.mini_tiles[p].area_id()
    }
}

/// What [`Map::area`] takes: an [`AreaId`], or the position of a tile or a
/// mini tile.
pub trait AreaKey {
    #[doc(hidden)]
    fn area_id_in(self, map: &Map) -> Option<i16>;
}

impl AreaKey for AreaId {
    fn area_id_in(self, map: &Map) -> Option<i16> {
        (self.get() as usize <= map.areas.len()).then_some(self.get())
    }
}

impl AreaKey for WalkPosition {
    fn area_id_in(self, map: &Map) -> Option<i16> {
        let id = map.mini_tiles.get(self)?.area_id();
        (id > 0).then_some(id)
    }
}

impl AreaKey for TilePosition {
    fn area_id_in(self, map: &Map) -> Option<i16> {
        let id = map.tiles.get(self)?.area_id;
        (id > 0).then_some(id)
    }
}

/// `TempAreaInfo` of `ComputeAreas`: an area being computed.
struct TempAreaInfo {
    valid: bool,
    id: i16,
    top: WalkPosition,
    highest_altitude: Altitude,
    size: i32,
}

impl Map {
    /// Analyses the map: `Map::Initialize`. Bases for start locations are found
    /// by a separate call of [`Map::find_bases_for_starting_locations`].
    pub fn initialize(input: &MapInput) -> Result<Map, InputError> {
        let size = TilePosition::new(input.buildable.width(), input.buildable.height());
        check_input(input, size)?;

        let walk_size = WalkPosition::from(size);
        let mut map = Map {
            size,
            walk_size,
            center: Position::from(size) / 2,
            tiles: Grid::filled(size.x, size.y, TileData::default()),
            mini_tiles: Grid::filled(walk_size.x, walk_size.y, MiniTile::default()),
            max_altitude: 0,
            automatic_path_update: false,
            neutrals: Vec::new(),
            minerals: Vec::new(),
            geysers: Vec::new(),
            static_buildings: Vec::new(),
            starting_locations: input.start_locations.clone(),
            raw_frontier: Vec::new(),
            areas: Vec::new(),
            choke_points: Vec::new(),
            choke_point_list: Vec::new(),
            cp_matrix: Vec::new(),
            cp_distances: Vec::new(),
            cp_paths: Vec::new(),
            path_back_trace: Vec::new(),
            bases: Vec::new(),
            base_count: 0,
        };
        let mut scratch = Scratch::new(size);

        map.load_data(input);
        map.decide_seas_or_lakes();
        map.initialize_neutrals(input);
        map.compute_altitude();
        map.process_blocking_neutrals();
        map.compute_areas();
        map.create_choke_points();
        map.compute_choke_point_distance_matrix(&mut scratch);
        map.collect_information();
        map.create_bases(&mut scratch);
        Ok(map)
    }

    /// Mini tiles: unwalkable in BWAPI makes the 8 neighbours unwalkable; every
    /// mini tile of a buildable or fully walkable tile is walkable. Tiles:
    /// buildable, ground height and doodad. `LoadData`.
    fn load_data(&mut self, input: &MapInput) {
        for y in 0..self.walk_size.y {
            for x in 0..self.walk_size.x {
                if !input.walkable[WalkPosition::new(x, y)] {
                    for dy in -1..=1 {
                        for dx in -1..=1 {
                            let w = WalkPosition::new(x + dx, y + dy);
                            if self.valid(w) {
                                self.mini_tiles[w].set_walkable(false);
                            }
                        }
                    }
                }
            }
        }

        for y in 0..self.size.y {
            for x in 0..self.size.x {
                let t = TilePosition::new(x, y);
                let buildable = input.buildable[t];
                if buildable {
                    self.tiles[t].buildable = true;
                }

                let mut walkable = true;
                for dy in 0..4 {
                    for dx in 0..4 {
                        if !input.walkable[WalkPosition::from(t) + WalkPosition::new(dx, dy)] {
                            walkable = false;
                        }
                    }
                }

                if buildable || walkable {
                    for dy in 0..4 {
                        for dx in 0..4 {
                            let w = WalkPosition::from(t) + WalkPosition::new(dx, dy);
                            self.mini_tiles[w].set_walkable(true);
                        }
                    }
                }

                let height = input.ground_height[t];
                self.tiles[t].ground_height = height / 2;
                if height % 2 != 0 {
                    self.tiles[t].doodad = true;
                }
            }
        }
    }

    /// Each 4-connected zone of unwalkable mini tiles is a sea, or a lake when it
    /// is small and away from the edges. `DecideSeasOrLakes`.
    fn decide_seas_or_lakes(&mut self) {
        for y in 0..self.walk_size.y {
            for x in 0..self.walk_size.x {
                let origin = WalkPosition::new(x, y);
                if !self.mini_tiles[origin].sea_or_lake() {
                    continue;
                }
                let mut to_search = vec![origin];
                let mut sea_extent = vec![origin];
                self.mini_tiles[origin].set_sea();
                let mut top_left = origin;
                let mut bottom_right = origin;
                while let Some(&current) = to_search.last() {
                    top_left.x = top_left.x.min(current.x);
                    top_left.y = top_left.y.min(current.y);
                    bottom_right.x = bottom_right.x.max(current.x);
                    bottom_right.y = bottom_right.y.max(current.y);

                    to_search.pop();
                    for (dx, dy) in DIRS4 {
                        let next = current + WalkPosition::new(dx, dy);
                        if self.valid(next) && self.mini_tiles[next].sea_or_lake() {
                            to_search.push(next);
                            if sea_extent.len() <= detail::LAKE_MAX_MINI_TILES {
                                sea_extent.push(next);
                            }
                            self.mini_tiles[next].set_sea();
                        }
                    }
                }

                if sea_extent.len() <= detail::LAKE_MAX_MINI_TILES
                    && bottom_right.x - top_left.x <= detail::LAKE_MAX_WIDTH_IN_MINI_TILES
                    && bottom_right.y - top_left.y <= detail::LAKE_MAX_WIDTH_IN_MINI_TILES
                    && top_left.x >= 2
                    && top_left.y >= 2
                    && bottom_right.x < self.walk_size.x - 2
                    && bottom_right.y < self.walk_size.y - 2
                {
                    for w in sea_extent {
                        self.mini_tiles[w].set_lake();
                    }
                }
            }
        }
    }

    /// `InitializeNeutrals`, on neutrals already classified.
    fn initialize_neutrals(&mut self, input: &MapInput) {
        for n in &input.neutrals {
            if n.kind == NeutralKind::Other {
                continue;
            }
            let index = self.neutrals.len() as u32;
            self.neutrals.push(NeutralData {
                kind: n.kind,
                unit_id: n.unit_id,
                type_id: n.type_id,
                pos: n.pos,
                top_left: n.top_left,
                size: n.size,
                initial_amount: n.initial_amount,
                next_stacked: None,
                blocked_areas: Vec::new(),
                alive: true,
            });
            match n.kind {
                NeutralKind::Mineral => self.minerals.push(index),
                NeutralKind::Geyser => self.geysers.push(index),
                NeutralKind::StaticBuilding => self.static_buildings.push(index),
                NeutralKind::Other => unreachable!(),
            }
            self.put_on_tiles(index);
        }
    }

    /// The altitude of every mini tile: its distance in pixels to the nearest sea
    /// mini tile or the edge of the map, by Dijkstra from the sea side.
    /// `ComputeAltitude`.
    fn compute_altitude(&mut self) {
        // 8 gives altitudes in pixels: mini tiles are 8×8 pixels.
        const ALTITUDE_SCALE: i32 = 8;

        // 1) The deltas by ascending altitude; others are obtained by symmetry.
        let range = self.walk_size.x.max(self.walk_size.y) / 2 + 3;
        let mut deltas: Vec<(WalkPosition, Altitude)> = Vec::new();
        for dy in 0..=range {
            for dx in dy..=range {
                if dx != 0 || dy != 0 {
                    let altitude = (0.5 + norm(dx, dy) * f64::from(ALTITUDE_SCALE)) as Altitude;
                    deltas.push((WalkPosition::new(dx, dy), altitude));
                }
            }
        }
        // The order of equal altitudes matters: it decides which sea side mini
        // tile marks a mini tile, and so when that one is dropped.
        stdsort::sort(&mut deltas, |a, b| a.1 < b.1);

        // 2) The sea side mini tiles, and the mini tiles around the map.
        let mut active: Vec<(WalkPosition, Altitude)> = Vec::new();
        for y in -1..=self.walk_size.y {
            for x in -1..=self.walk_size.x {
                let w = WalkPosition::new(x, y);
                if !self.valid(w) || self.sea_side(w) {
                    active.push((w, 0));
                }
            }
        }

        // 3) Dijkstra.
        for &(d, altitude) in &deltas {
            let mut i = 0;
            while i < active.len() {
                let (origin, last) = active[i];
                if i32::from(altitude) - i32::from(last) >= 2 * ALTITUDE_SCALE {
                    // It will not give least altitudes any more.
                    active.swap_remove(i);
                    continue;
                }
                for (dx, dy) in [
                    (d.x, d.y),
                    (-d.x, d.y),
                    (d.x, -d.y),
                    (-d.x, -d.y),
                    (d.y, d.x),
                    (-d.y, d.x),
                    (d.y, -d.x),
                    (-d.y, -d.x),
                ] {
                    let w = origin + WalkPosition::new(dx, dy);
                    if self.valid(w) && self.mini_tiles[w].altitude_missing() {
                        self.max_altitude = altitude;
                        active[i].1 = altitude;
                        self.mini_tiles[w].set_altitude(altitude);
                    }
                }
                i += 1;
            }
        }
    }

    /// `utils::seaSide`: a sea mini tile next to a mini tile that is not sea.
    fn sea_side(&self, p: WalkPosition) -> bool {
        if !self.mini_tiles[p].sea() {
            return false;
        }
        DIRS4.iter().any(|&(dx, dy)| {
            let next = p + WalkPosition::new(dx, dy);
            self.valid(next) && !self.mini_tiles[next].sea()
        })
    }

    /// A walkable mini tile next to a lake or a neutral.
    fn adjoins8_some_lake_or_neutral(&self, p: WalkPosition) -> bool {
        DIRS8.iter().any(|&(dx, dy)| {
            let next = p + WalkPosition::new(dx, dy);
            self.valid(next)
                && (self.tiles[TilePosition::from(next)].neutral.is_some()
                    || self.mini_tiles[next].lake())
        })
    }

    fn walkable_free(&self, w: WalkPosition) -> bool {
        self.mini_tiles[w].walkable() && self.tiles[TilePosition::from(w)].neutral.is_none()
    }

    /// A static building or a mineral with two or more true doors blocks: its
    /// mini tiles get no area, so the areas at its doors stay apart.
    /// `ProcessBlockingNeutrals`.
    fn process_blocking_neutrals(&mut self) {
        let candidates: Vec<u32> = self
            .static_buildings
            .iter()
            .chain(&self.minerals)
            .copied()
            .collect();

        for candidate in candidates {
            // With stacked neutrals, only the top one.
            if self.neutrals[candidate as usize].next_stacked.is_some() {
                continue;
            }
            let NeutralData { top_left, size, .. } = self.neutrals[candidate as usize];

            // 1) The outer border of the candidate.
            let mut border = outer_mini_tile_border(top_left, size);
            border.retain(|&w| self.valid(w) && self.walkable_free(w));

            // 2) The doors: one per connected set of walkable border mini tiles
            //    next to some lake or neutral.
            let mut doors = Vec::new();
            while let Some(door) = border.pop() {
                doors.push(door);
                let mut to_visit = vec![door];
                let mut visited = vec![door];
                while let Some(current) = to_visit.pop() {
                    for (dx, dy) in DIRS4 {
                        let next = current + WalkPosition::new(dx, dy);
                        if self.valid(next)
                            && !visited.contains(&next)
                            && self.walkable_free(next)
                            && self.adjoins8_some_lake_or_neutral(next)
                        {
                            to_visit.push(next);
                            visited.push(next);
                        }
                    }
                }
                border.retain(|w| !visited.contains(w));
            }

            // 3) The true doors: the doors that give onto a big enough zone.
            let mut true_doors = Vec::new();
            if doors.len() >= 2 {
                let limit = if self.neutrals[candidate as usize].kind == NeutralKind::StaticBuilding
                {
                    10
                } else {
                    400
                };
                for &door in &doors {
                    let mut to_visit = vec![door];
                    let mut visited = vec![door];
                    while visited.len() < limit {
                        let Some(current) = to_visit.pop() else {
                            break;
                        };
                        for (dx, dy) in DIRS4 {
                            let next = current + WalkPosition::new(dx, dy);
                            if self.valid(next)
                                && !visited.contains(&next)
                                && self.walkable_free(next)
                            {
                                to_visit.push(next);
                                visited.push(next);
                            }
                        }
                    }
                    if visited.len() >= limit {
                        true_doors.push(door);
                    }
                }
            }

            // 4) Two true doors: the candidate, and its whole stack, block.
            if true_doors.len() >= 2 {
                let mut next = self.tiles[top_left].neutral;
                while let Some(n) = next {
                    self.neutrals[n as usize].blocked_areas = true_doors.clone();
                    next = self.neutrals[n as usize].next_stacked;
                }

                let walk_size = WalkPosition::from(size);
                for dy in 0..walk_size.y {
                    for dx in 0..walk_size.x {
                        let w = WalkPosition::from(top_left) + WalkPosition::new(dx, dy);
                        if self.mini_tiles[w].walkable() {
                            self.mini_tiles[w].set_blocked();
                        }
                    }
                }
            }
        }
    }

    /// Mini tiles in descending order of altitude each start an area, join their
    /// neighbouring area, or make two neighbouring areas merge or meet at a
    /// frontier. `ComputeAreas`.
    fn compute_areas(&mut self) {
        let by_descending_altitude = self.sort_mini_tiles();
        let temp_areas = self.compute_temp_areas(&by_descending_altitude);
        self.create_areas(&temp_areas);
        self.set_area_id_in_tiles();
    }

    /// The mini tiles without an area, by descending altitude; equal altitudes
    /// in the order `std::sort` of libstdc++ leaves them.
    fn sort_mini_tiles(&self) -> Vec<WalkPosition> {
        let mut result = Vec::new();
        for y in 0..self.walk_size.y {
            for x in 0..self.walk_size.x {
                let w = WalkPosition::new(x, y);
                if self.mini_tiles[w].area_id_missing() {
                    result.push(w);
                }
            }
        }
        let mini_tiles = &self.mini_tiles;
        stdsort::sort(&mut result, |a, b| {
            mini_tiles[*a].altitude() > mini_tiles[*b].altitude()
        });
        result
    }

    /// `findNeighboringAreas`: the first area id around, and the least other one.
    fn find_neighboring_areas(&self, p: WalkPosition) -> (i16, i16) {
        let mut result = (0, 0);
        for (dx, dy) in DIRS4 {
            let next = p + WalkPosition::new(dx, dy);
            if self.valid(next) {
                let area_id = self.mini_tiles[next].area_id();
                if area_id > 0 {
                    if result.0 == 0 {
                        result.0 = area_id;
                    } else if result.0 != area_id && (result.1 == 0 || area_id < result.1) {
                        result.1 = area_id;
                    }
                }
            }
        }
        result
    }

    fn compute_temp_areas(&mut self, by_descending_altitude: &[WalkPosition]) -> Vec<TempAreaInfo> {
        // Index 0 unused: area ids are > 0.
        let mut temp_areas = vec![TempAreaInfo {
            valid: false,
            id: 0,
            top: WalkPosition::new(0, 0),
            highest_altitude: 0,
            size: 0,
        }];
        // `chooseNeighboringArea`, with its counter local to the analysis.
        let mut pair_counter: BTreeMap<(i16, i16), i32> = BTreeMap::new();

        for &pos in by_descending_altitude {
            let altitude = self.mini_tiles[pos].altitude();
            let neighboring = self.find_neighboring_areas(pos);
            if neighboring.0 == 0 {
                // No neighbouring area: a new area.
                let id = temp_areas.len() as i16;
                temp_areas.push(TempAreaInfo {
                    valid: true,
                    id,
                    top: pos,
                    highest_altitude: altitude,
                    size: 1,
                });
                self.mini_tiles[pos].set_area_id(id);
            } else if neighboring.1 == 0 {
                // One neighbouring area: joins it.
                temp_areas[neighboring.0 as usize].size += 1;
                self.mini_tiles[pos].set_area_id(neighboring.0);
            } else {
                // Two neighbouring areas: joins one of them, and they may merge.
                let mut smaller = neighboring.0;
                let mut bigger = neighboring.1;
                if temp_areas[smaller as usize].size > temp_areas[bigger as usize].size {
                    core::mem::swap(&mut smaller, &mut bigger);
                }
                let small = &temp_areas[smaller as usize];
                let big = &temp_areas[bigger as usize];
                let near_start = self.starting_locations.iter().any(|&start| {
                    dist(TilePosition::from(pos), start + TilePosition::new(2, 1)) <= 3.0
                });
                let merge = small.size < 80
                    || small.highest_altitude < 80
                    || f64::from(altitude) / f64::from(big.highest_altitude) >= 0.90
                    || f64::from(altitude) / f64::from(small.highest_altitude) >= 0.90
                    || near_start;

                if merge {
                    temp_areas[bigger as usize].size += 1;
                    self.mini_tiles[pos].set_area_id(bigger);

                    let top = temp_areas[smaller as usize].top;
                    self.replace_area_ids(top, bigger);
                    let absorbed = temp_areas[smaller as usize].size;
                    temp_areas[bigger as usize].size += absorbed;
                    temp_areas[smaller as usize].valid = false;
                } else {
                    // The frontier between the two areas starts or goes on.
                    let (a, b) = if smaller > bigger {
                        (bigger, smaller)
                    } else {
                        (smaller, bigger)
                    };
                    let counter = pair_counter.entry((a, b)).or_insert(0);
                    let chosen = if *counter % 2 == 0 { a } else { b };
                    *counter += 1;
                    temp_areas[chosen as usize].size += 1;
                    self.mini_tiles[pos].set_area_id(chosen);
                    self.raw_frontier.push((neighboring, pos));
                }
            }
        }

        // Positions between parts of one area are no frontier.
        self.raw_frontier.retain(|&((a, b), _)| a != b);
        temp_areas
    }

    /// `ReplaceAreaIds`: the area of `p` gets `new_id`, in the mini tiles and in
    /// the raw frontier.
    fn replace_area_ids(&mut self, p: WalkPosition, new_id: i16) {
        let old_id = self.mini_tiles[p].area_id();
        self.mini_tiles[p].replace_area_id(new_id);

        let mut to_search = vec![p];
        while let Some(current) = to_search.pop() {
            for (dx, dy) in DIRS4 {
                let next = current + WalkPosition::new(dx, dy);
                if self.valid(next) && self.mini_tiles[next].area_id() == old_id {
                    to_search.push(next);
                    self.mini_tiles[next].replace_area_id(new_id);
                }
            }
        }

        if new_id > 0 {
            for ((a, b), _) in &mut self.raw_frontier {
                if *a == old_id {
                    *a = new_id;
                }
                if *b == old_id {
                    *b = new_id;
                }
            }
        }
    }

    /// Areas of at least `AREA_MIN_MINI_TILES` get the ids 1.., smaller ones -2
    /// and down. `CreateAreas`.
    fn create_areas(&mut self, temp_areas: &[TempAreaInfo]) {
        let mut areas_list: Vec<(WalkPosition, i32)> = Vec::new();
        let mut new_area_id: i16 = 1;
        let mut new_tiny_area_id: i16 = -2;

        for temp in temp_areas.iter().filter(|t| t.valid) {
            if temp.size >= detail::AREA_MIN_MINI_TILES {
                if new_area_id != temp.id {
                    self.replace_area_ids(temp.top, new_area_id);
                }
                areas_list.push((temp.top, temp.size));
                new_area_id += 1;
            } else {
                self.replace_area_ids(temp.top, new_tiny_area_id);
                new_tiny_area_id -= 1;
            }
        }

        self.graph_create_areas(&areas_list);
    }

    /// `SetAreaIdInTile`: the area of its walkable mini tiles if they share one,
    /// -1 if they do not.
    pub(crate) fn set_area_id_in_tile(&mut self, t: TilePosition) {
        for dy in 0..4 {
            for dx in 0..4 {
                let id =
                    self.mini_tiles[WalkPosition::from(t) + WalkPosition::new(dx, dy)].area_id();
                if id != 0 {
                    if self.tiles[t].area_id == 0 {
                        self.tiles[t].area_id = id;
                    } else if self.tiles[t].area_id != id {
                        self.tiles[t].area_id = -1;
                        return;
                    }
                }
            }
        }
    }

    fn set_altitude_in_tile(&mut self, t: TilePosition) {
        let mut min_altitude = Altitude::MAX;
        for dy in 0..4 {
            for dx in 0..4 {
                let altitude =
                    self.mini_tiles[WalkPosition::from(t) + WalkPosition::new(dx, dy)].altitude();
                min_altitude = min_altitude.min(altitude);
            }
        }
        self.tiles[t].min_altitude = min_altitude;
    }

    fn set_area_id_in_tiles(&mut self) {
        for y in 0..self.size.y {
            for x in 0..self.size.x {
                let t = TilePosition::new(x, y);
                self.set_area_id_in_tile(t);
                self.set_altitude_in_tile(t);
            }
        }
    }

    /// Assigns each start location to the base at most 3 tiles away (Chebyshev),
    /// which moves there. False if a start location gets no base.
    /// `FindBasesForStartingLocations`.
    pub fn find_bases_for_starting_locations(&mut self) -> bool {
        let mut at_least_one_failed = false;
        for i in 0..self.starting_locations.len() {
            let location = self.starting_locations[i];
            let mut found = false;
            'areas: for area in &self.areas {
                for &base in &area.bases {
                    let base = &mut self.bases[base as usize];
                    if base.location.chebyshev(location)
                        <= detail::MAX_TILES_BETWEEN_STARTING_LOCATION_AND_ITS_ASSIGNED_BASE
                    {
                        base.set_starting_location(location);
                        found = true;
                        break 'areas;
                    }
                }
            }
            if !found {
                at_least_one_failed = true;
            }
        }
        !at_least_one_failed
    }

    /// Whether choke points blocked by a neutral open, and distances are
    /// computed again, when the neutral is destroyed.
    pub fn automatic_path_update(&self) -> bool {
        self.automatic_path_update
    }

    pub fn enable_automatic_path_analysis(&mut self) {
        self.automatic_path_update = true;
    }

    /// The size in tiles.
    pub fn size(&self) -> TilePosition {
        self.size
    }

    /// The size in mini tiles.
    pub fn walk_size(&self) -> WalkPosition {
        self.walk_size
    }

    pub fn center(&self) -> Position {
        self.center
    }

    /// The greatest altitude of a mini tile.
    pub fn max_altitude(&self) -> Altitude {
        self.max_altitude
    }

    pub fn base_count(&self) -> usize {
        self.base_count
    }

    pub fn choke_point_count(&self) -> usize {
        self.choke_point_list.len()
    }

    pub fn tile(&self, t: TilePosition) -> Option<Tile<'_>> {
        self.tiles.get(t).map(|data| Tile { map: self, data })
    }

    pub fn mini_tile(&self, w: WalkPosition) -> Option<MiniTile> {
        self.mini_tiles.get(w).copied()
    }

    /// Every tile, row by row.
    pub fn tiles(&self) -> impl Iterator<Item = Tile<'_>> {
        self.tiles
            .cells()
            .iter()
            .map(move |data| Tile { map: self, data })
    }

    /// Every mini tile, row by row.
    pub fn mini_tiles(&self) -> impl Iterator<Item = MiniTile> + '_ {
        self.mini_tiles.cells().iter().copied()
    }

    pub fn valid<P: MapPosition>(&self, p: P) -> bool {
        p.valid_in(self)
    }

    /// The nearest position inside the map.
    pub fn crop<P: MapPosition>(&self, p: P) -> P {
        p.crop_in(self)
    }

    pub fn starting_locations(&self) -> &[TilePosition] {
        &self.starting_locations
    }

    pub fn minerals(&self) -> impl Iterator<Item = Mineral<'_>> {
        self.minerals
            .iter()
            .map(move |&i| Mineral(Neutral::new(self, i)))
    }

    pub fn geysers(&self) -> impl Iterator<Item = Geyser<'_>> {
        self.geysers
            .iter()
            .map(move |&i| Geyser(Neutral::new(self, i)))
    }

    pub fn static_buildings(&self) -> impl Iterator<Item = StaticBuilding<'_>> {
        self.static_buildings
            .iter()
            .map(move |&i| StaticBuilding(Neutral::new(self, i)))
    }

    /// The mineral of the unit `unit_id`.
    pub fn mineral_by_unit(&self, unit_id: i32) -> Option<Mineral<'_>> {
        self.minerals().find(|m| m.unit_id() == unit_id)
    }

    /// The geyser of the unit `unit_id`.
    pub fn geyser_by_unit(&self, unit_id: i32) -> Option<Geyser<'_>> {
        self.geysers().find(|g| g.unit_id() == unit_id)
    }

    /// Removes the mineral of the unit `unit_id` from the map, its areas and its
    /// bases. False if there is no such mineral.
    pub fn on_mineral_destroyed(&mut self, unit_id: i32) -> bool {
        let Some(i) = self
            .minerals
            .iter()
            .position(|&m| self.neutrals[m as usize].unit_id == unit_id)
        else {
            return false;
        };
        let mineral = self.minerals.swap_remove(i);
        // `~Mineral`, then `~Neutral`.
        for area in 0..self.areas.len() {
            self.area_on_mineral_destroyed(area, mineral);
        }
        self.destroy_neutral(mineral);
        true
    }

    /// Removes the static building of the unit `unit_id`. False if there is no
    /// such static building.
    pub fn on_static_building_destroyed(&mut self, unit_id: i32) -> bool {
        let Some(i) = self
            .static_buildings
            .iter()
            .position(|&s| self.neutrals[s as usize].unit_id == unit_id)
        else {
            return false;
        };
        let building = self.static_buildings.swap_remove(i);
        self.destroy_neutral(building);
        true
    }

    /// `~Neutral`.
    fn destroy_neutral(&mut self, index: u32) {
        self.remove_from_tiles(index);
        if !self.neutrals[index as usize].blocked_areas.is_empty() {
            self.on_blocking_neutral_destroyed(index);
        }
        self.neutrals[index as usize].alive = false;
    }

    /// `OnBlockingNeutralDestroyed`: once no neutral is left on its tiles, the
    /// mini tiles of the neutral join the first area it separated.
    fn on_blocking_neutral_destroyed(&mut self, blocking: u32) {
        let blocked_areas: Vec<i16> = Neutral::new(self, blocking)
            .blocked_areas()
            .map(|a| a.id().get())
            .collect();

        for &area in &blocked_areas {
            for i in 0..self.areas[area as usize - 1].choke_points.len() {
                let cp = self.areas[area as usize - 1].choke_points[i];
                self.cp_on_blocking_neutral_destroyed(cp, blocking);
            }
        }

        let NeutralData { top_left, size, .. } = self.neutrals[blocking as usize];
        if self.tiles[top_left].neutral.is_some() {
            // Some blocking neutrals remain at the same location.
            return;
        }

        if let Some(&new_id) = blocked_areas.first() {
            let walk_size = WalkPosition::from(size);
            for dy in 0..walk_size.y {
                for dx in 0..walk_size.x {
                    let w = WalkPosition::from(top_left) + WalkPosition::new(dx, dy);
                    if self.mini_tiles[w].walkable() {
                        self.mini_tiles[w].replace_blocked_area_id(new_id);
                    }
                }
            }
        }

        for dy in 0..size.y {
            for dx in 0..size.x {
                let t = top_left + TilePosition::new(dx, dy);
                self.tiles[t].area_id = 0;
                self.set_area_id_in_tile(t);
            }
        }

        if self.automatic_path_update {
            self.compute_choke_point_distance_matrix(&mut Scratch::new(self.size));
        }
    }

    pub fn areas(&self) -> impl ExactSizeIterator<Item = Area<'_>> {
        (1..=self.areas.len() as i16).map(move |id| Area::new(self, id))
    }

    /// The area of an [`AreaId`], of a mini tile or of a tile. Prefer
    /// [`Map::nearest_area`] for positions: lakes and tiles shared by areas have
    /// none.
    pub fn area(&self, key: impl AreaKey) -> Option<Area<'_>> {
        key.area_id_in(self).map(|id| Area::new(self, id))
    }

    /// The area of the position, or else of the nearest tile or mini tile that
    /// has one.
    pub fn nearest_area<P: CellPosition>(&self, p: P) -> Option<Area<'_>> {
        if !self.valid(p) {
            return None;
        }
        let id = P::area_id(self, p);
        if id > 0 {
            return Some(Area::new(self, id));
        }
        let found = self.bfs(p, |map, q| P::area_id(map, q) > 0, |_, _| true, true)?;
        Some(Area::new(self, P::area_id(self, found)))
    }

    /// The choke points from `a` to `b` through the graph of choke points: empty
    /// when both are in one area or when `b` is unreachable from `a`.
    pub fn choke_point_path(&self, a: Position, b: Position) -> CpPath<'_> {
        self.choke_point_path_with_length(a, b).0
    }

    /// [`Map::choke_point_path`] and the approximate ground distance; no
    /// distance when unreachable.
    pub fn choke_point_path_with_length(
        &self,
        a: Position,
        b: Position,
    ) -> (CpPath<'_>, Option<i32>) {
        let (path, length) = self.graph_get_path(a, b);
        (CpPath::new(self, path), length)
    }

    /// The first position from `start` on, in breadth first order, whose cell
    /// meets `find`; the search goes only through cells that meet `visit`.
    /// `None` if there is none (BWEM asserts and returns `start`).
    pub fn breadth_first_search<'m, P: CellPosition>(
        &'m self,
        start: P,
        mut find: impl FnMut(P::Cell<'m>, P) -> bool,
        mut visit: impl FnMut(P::Cell<'m>, P) -> bool,
        connect8: bool,
    ) -> Option<P> {
        self.bfs(
            start,
            |map, p| find(P::cell(map, p), p),
            |map, p| visit(P::cell(map, p), p),
            connect8,
        )
    }

    pub(crate) fn bfs<'m, P: CellPosition>(
        &'m self,
        start: P,
        mut find: impl FnMut(&'m Map, P) -> bool,
        mut visit: impl FnMut(&'m Map, P) -> bool,
        connect8: bool,
    ) -> Option<P> {
        if !self.valid(start) {
            return None;
        }
        if find(self, start) {
            return Some(start);
        }
        let mut visited = vec![false; P::grid_len(self)];
        let mut to_visit = VecDeque::new();
        to_visit.push_back(start);
        visited[P::grid_index(self, start)] = true;

        let directions: &[(i32, i32)] = if connect8 { &DIRS8 } else { &DIRS4 };
        while let Some(current) = to_visit.pop_front() {
            for &(dx, dy) in directions {
                let next = current.offset(dx, dy);
                if self.valid(next) {
                    if find(self, next) {
                        return Some(next);
                    }
                    let i = P::grid_index(self, next);
                    if visit(self, next) && !visited[i] {
                        to_visit.push_back(next);
                        visited[i] = true;
                    }
                }
            }
        }
        None
    }

    /// The positions where two areas meet, with the ids of the two areas, in
    /// descending order of altitude.
    pub fn raw_frontier(&self) -> &[((i16, i16), WalkPosition)] {
        &self.raw_frontier
    }

    pub fn choke_point(&self, id: ChokePointId) -> Option<ChokePoint<'_>> {
        ((id.0 as usize) < self.choke_points.len()).then(|| ChokePoint::new(self, id.0))
    }

    pub fn base(&self, id: BaseId) -> Option<Base<'_>> {
        ((id.0 as usize) < self.bases.len()).then(|| Base::new(self, id.0))
    }

    /// The neutral, unless it was destroyed.
    pub fn neutral(&self, id: NeutralId) -> Option<Neutral<'_>> {
        self.neutrals
            .get(id.0 as usize)
            .filter(|n| n.alive)
            .map(|_| Neutral::new(self, id.0))
    }

    pub(crate) fn cp_distance(&self, a: u32, b: u32) -> i32 {
        self.cp_distances[a as usize * self.choke_points.len() + b as usize]
    }

    pub(crate) fn cp_path(&self, a: u32, b: u32) -> &[u32] {
        &self.cp_paths[a as usize * self.choke_points.len() + b as usize]
    }
}

fn check_input(input: &MapInput, size: TilePosition) -> Result<(), InputError> {
    if size.x == 0 || size.y == 0 {
        return Err(InputError::EmptyMap);
    }
    let grids = [
        (
            "walkable",
            input.walkable.width(),
            input.walkable.height(),
            4,
        ),
        (
            "ground_height",
            input.ground_height.width(),
            input.ground_height.height(),
            1,
        ),
    ];
    for (grid, width, height, scale) in grids {
        if width != size.x * scale || height != size.y * scale {
            return Err(InputError::GridSize {
                grid,
                width,
                height,
            });
        }
    }
    for y in 0..size.y {
        for x in 0..size.x {
            let tile = TilePosition::new(x, y);
            let height = input.ground_height[tile];
            if height > 5 {
                return Err(InputError::GroundHeight { tile, height });
            }
        }
    }
    for n in input
        .neutrals
        .iter()
        .filter(|n| n.kind != NeutralKind::Other)
    {
        let bottom_right = n.top_left + n.size;
        if n.size.x < 1
            || n.size.y < 1
            || n.top_left.x < 0
            || n.top_left.y < 0
            || bottom_right.x > size.x
            || bottom_right.y > size.y
        {
            return Err(InputError::NeutralOutside { unit_id: n.unit_id });
        }
    }
    Ok(())
}
