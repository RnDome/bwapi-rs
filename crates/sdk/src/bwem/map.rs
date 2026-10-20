// Port of BWEM/include/map.h, BWEM/include/mapImpl.h, BWEM/src/map.cpp and
// BWEM/src/mapImpl.cpp of BWEM-community
// (https://github.com/N00byEdge/BWEM-community, 9a63141).
// Copyright (c) 2015, 2017, Igor Dimitrijevic. MIT/X11 License, see LICENSE-BWEM.

//! The map: its analysis, its queries and its updates.

use alloc::collections::{BTreeMap, VecDeque};
use alloc::vec;
use alloc::vec::Vec;

use bwapi::{Position, TilePosition, WalkPosition};

use super::area::{AreaState, StaticArea};
use super::base::{BaseState, StaticBase};
use super::cp::{ChokePointState, StaticChokePoint};
use super::geometry_helpers::{distance, norm, outer_mini_tile_border};
use super::neutral::{NeutralState, StaticNeutral};
use super::tiles::{TileData, TileState, TileTerrain};
use super::{
    Altitude, Area, AreaId, Base, BaseId, ChokePoint, ChokePointId, ChokePointPath, InputError,
    MapInput, MiniTile, Neutral, NeutralId, NeutralKind, Tile, detail,
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

/// The analysed map: `BWEM::Map`. What never changes after the analysis, and
/// what changes when neutrals are destroyed.
#[derive(Clone, Debug)]
pub struct Map {
    pub(crate) geometry: Geometry,
    pub(crate) state: State,
}

/// What never changes after the analysis: the altitudes and the terrain of
/// tiles, the areas, choke points and bases, every neutral there ever was.
#[derive(Clone, Debug)]
pub struct Geometry {
    pub(crate) size: TilePosition,
    pub(crate) walk_size: WalkPosition,
    pub(crate) center: Position,
    pub(crate) max_altitude: Altitude,
    pub(crate) mini_tile_altitudes: Grid<WalkPosition, Altitude>,
    pub(crate) tiles: Grid<TilePosition, TileTerrain>,
    pub(crate) starting_locations: Vec<TilePosition>,
    pub(crate) raw_frontier: Vec<((i16, i16), WalkPosition)>,

    /// Every neutral ever created; a destroyed one stays, not alive.
    pub(crate) neutrals: Vec<StaticNeutral>,
    pub(crate) geysers: Vec<u32>,

    /// Area `id` at `id - 1`.
    pub(crate) areas: Vec<StaticArea>,
    /// By index.
    pub(crate) choke_points: Vec<StaticChokePoint>,
    /// `Graph::ChokePoints()`: in the order of the pairs of areas.
    pub(crate) choke_point_traversal: Vec<u32>,
    pub(crate) bases: Vec<StaticBase>,
    pub(crate) base_count: usize,
}

/// What changes when neutrals are destroyed: the areas of mini tiles and
/// tiles, the neutrals left and their stacks, blocked choke points, distances
/// and paths between choke points, groups, live minerals.
#[derive(Clone, Debug)]
pub struct State {
    pub(crate) mini_tile_areas: Grid<WalkPosition, i16>,
    pub(crate) tiles: Grid<TilePosition, TileState>,
    pub(crate) automatic_path_update: bool,

    /// By index, as `Geometry::neutrals`.
    pub(crate) neutrals: Vec<NeutralState>,
    /// The minerals and static buildings not destroyed.
    pub(crate) minerals: Vec<u32>,
    pub(crate) static_buildings: Vec<u32>,

    /// As `Geometry::areas`.
    pub(crate) areas: Vec<AreaState>,
    /// As `Geometry::choke_points`.
    pub(crate) choke_points: Vec<ChokePointState>,
    /// `choke_points.len()²` distances, -1 when unreachable.
    pub(crate) choke_point_distances: Vec<i32>,
    pub(crate) choke_point_paths: Vec<Vec<u32>>,
    /// `ChokePoint::PathBackTrace`, kept between computations as in C++.
    pub(crate) path_back_trace: Vec<Option<u32>>,
    /// As `Geometry::bases`.
    pub(crate) bases: Vec<BaseState>,
}

/// What [`Map::remove_neutral`] changed, by meaning.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Changes {
    /// The neutral that is gone.
    pub neutral: NeutralId,
    /// Its tiles: the only ones whose area may differ now.
    pub tiles: TileRect,
    /// The choke points it no longer blocks.
    pub unblocked: Vec<ChokePointId>,
    /// Whether connectivity or distances between areas may differ: groups,
    /// accessible neighbours, distances and paths between choke points.
    pub topology_changed: bool,
}

/// Tiles from `top_left`, `size` wide and high: as a neutral gives its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TileRect {
    pub top_left: TilePosition,
    pub size: TilePosition,
}

/// The geometry and the state of a map, as its views read them.
#[derive(Clone, Copy)]
pub(crate) struct MapView<'m> {
    pub(crate) geometry: &'m Geometry,
    pub(crate) state: &'m State,
}

/// The first steps of the analysis, up to the areas: they write mini tiles and
/// tiles whole, which [`Work::finish`] then splits into the geometry and the
/// state of the map.
struct Work {
    size: TilePosition,
    walk_size: WalkPosition,
    tiles: Grid<TilePosition, TileData>,
    mini_tiles: Grid<WalkPosition, MiniTile>,
    max_altitude: Altitude,
    neutrals: Vec<StaticNeutral>,
    neutral_states: Vec<NeutralState>,
    minerals: Vec<u32>,
    geysers: Vec<u32>,
    static_buildings: Vec<u32>,
    starting_locations: Vec<TilePosition>,
    raw_frontier: Vec<((i16, i16), WalkPosition)>,
    /// The top, its altitude and the size in mini tiles of each area.
    areas: Vec<(WalkPosition, Altitude, i32)>,
}

/// The scratch data of an analysis: `Tile::InternalData` and the marks of
/// `Markable`, which BWEM keeps in its objects.
pub(crate) struct Scratch {
    pub(crate) internal: Grid<TilePosition, i32>,
    /// The insertion order of a tile in a priority queue: entries of equal
    /// distance leave a `std::multimap` in insertion order.
    pub(crate) insertion_order: Grid<TilePosition, u64>,
    mark: Grid<TilePosition, u32>,
    current_mark: u32,
}

impl Scratch {
    pub(crate) fn new(size: TilePosition) -> Self {
        Scratch {
            internal: Grid::filled(size.x, size.y, 0),
            insertion_order: Grid::filled(size.x, size.y, 0),
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
    fn valid_in(self, map: &Map) -> bool {
        self.inside(&map.geometry)
    }
    fn crop_in(self, map: &Map) -> Self;
    #[doc(hidden)]
    fn inside(self, geometry: &Geometry) -> bool;
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
    fn inside(self, geometry: &Geometry) -> bool {
        let size = geometry.size;
        (0..size.x).contains(&self.x) && (0..size.y).contains(&self.y)
    }

    fn crop_in(self, map: &Map) -> Self {
        let size = map.geometry.size;
        let (x, y) = crop_xy(self.x, self.y, size.x, size.y);
        TilePosition::new(x, y)
    }
}

impl MapPosition for WalkPosition {
    fn inside(self, geometry: &Geometry) -> bool {
        valid_walk(self, geometry.walk_size)
    }

    fn crop_in(self, map: &Map) -> Self {
        let walk_size = map.geometry.walk_size;
        let (x, y) = crop_xy(self.x, self.y, walk_size.x, walk_size.y);
        WalkPosition::new(x, y)
    }
}

impl MapPosition for Position {
    fn inside(self, geometry: &Geometry) -> bool {
        WalkPosition::from(self).inside(geometry)
    }

    fn crop_in(self, map: &Map) -> Self {
        let size = map.geometry.size;
        let (x, y) = crop_xy(self.x, self.y, 32 * size.x, 32 * size.y);
        Position::new(x, y)
    }
}

fn valid_walk(w: WalkPosition, walk_size: WalkPosition) -> bool {
    (0..walk_size.x).contains(&w.x) && (0..walk_size.y).contains(&w.y)
}

/// A tile or a mini tile position: what a breadth first search and a nearest
/// area search walk on.
pub trait CellPosition: MapPosition + PartialEq {
    /// [`Tile`] for tile positions, [`MiniTile`] for walk positions.
    type Cell<'m>;

    #[doc(hidden)]
    fn cell<'m>(geometry: &'m Geometry, state: &'m State, p: Self) -> Self::Cell<'m>;
    #[doc(hidden)]
    fn offset(self, dx: i32, dy: i32) -> Self;
    #[doc(hidden)]
    fn grid_index(geometry: &Geometry, p: Self) -> usize;
    #[doc(hidden)]
    fn grid_len(geometry: &Geometry) -> usize;
    #[doc(hidden)]
    fn area_id(state: &State, p: Self) -> i16;
}

impl CellPosition for TilePosition {
    type Cell<'m> = Tile<'m>;

    fn cell<'m>(geometry: &'m Geometry, state: &'m State, p: Self) -> Tile<'m> {
        Tile::new(MapView { geometry, state }, p)
    }

    fn offset(self, dx: i32, dy: i32) -> Self {
        self + TilePosition::new(dx, dy)
    }

    fn grid_index(geometry: &Geometry, p: Self) -> usize {
        (p.y * geometry.size.x + p.x) as usize
    }

    fn grid_len(geometry: &Geometry) -> usize {
        (geometry.size.x * geometry.size.y) as usize
    }

    fn area_id(state: &State, p: Self) -> i16 {
        state.tiles[p].area_id
    }
}

impl CellPosition for WalkPosition {
    type Cell<'m> = MiniTile;

    fn cell<'m>(geometry: &'m Geometry, state: &'m State, p: Self) -> MiniTile {
        MapView { geometry, state }.mini_tile_at(p)
    }

    fn offset(self, dx: i32, dy: i32) -> Self {
        self + WalkPosition::new(dx, dy)
    }

    fn grid_index(geometry: &Geometry, p: Self) -> usize {
        (p.y * geometry.walk_size.x + p.x) as usize
    }

    fn grid_len(geometry: &Geometry) -> usize {
        (geometry.walk_size.x * geometry.walk_size.y) as usize
    }

    fn area_id(state: &State, p: Self) -> i16 {
        state.mini_tile_areas[p]
    }
}

/// `TempAreaInfo` of `ComputeAreas`: an area being computed.
struct TemporaryArea {
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
        let mut work = Work {
            size,
            walk_size,
            tiles: Grid::filled(size.x, size.y, TileData::default()),
            mini_tiles: Grid::filled(walk_size.x, walk_size.y, MiniTile::default()),
            max_altitude: 0,
            neutrals: Vec::new(),
            neutral_states: Vec::new(),
            minerals: Vec::new(),
            geysers: Vec::new(),
            static_buildings: Vec::new(),
            starting_locations: input.start_locations.clone(),
            raw_frontier: Vec::new(),
            areas: Vec::new(),
        };
        work.load_data(input);
        work.decide_seas_or_lakes();
        work.initialize_neutrals(input);
        work.compute_altitude();
        work.process_blocking_neutrals();
        work.compute_areas();

        let mut map = work.finish();
        let mut scratch = Scratch::new(size);
        map.create_choke_points();
        map.compute_choke_point_distance_matrix(&mut scratch);
        map.collect_information();
        map.create_bases(&mut scratch);
        map.record_initial_minerals();
        Ok(map)
    }

    /// The minerals of each area and base as of the analysis.
    fn record_initial_minerals(&mut self) {
        for (area, state) in self.geometry.areas.iter_mut().zip(&self.state.areas) {
            area.initial_minerals = state.minerals.iter().map(|&i| NeutralId(i)).collect();
        }
        for (base, state) in self.geometry.bases.iter_mut().zip(&self.state.bases) {
            base.initial_minerals = state.minerals.iter().map(|&i| NeutralId(i)).collect();
        }
    }
}

impl Work {
    fn valid(&self, w: WalkPosition) -> bool {
        valid_walk(w, self.walk_size)
    }

    /// `Neutral::PutOnTiles`: a neutral on a tile that has one joins the top of
    /// that stack.
    fn put_on_tiles(&mut self, index: u32) {
        let StaticNeutral { top_left, size, .. } = self.neutrals[index as usize];
        for dy in 0..size.y {
            for dx in 0..size.x {
                let t = top_left + TilePosition::new(dx, dy);
                match self.tiles[t].neutral {
                    None => self.tiles[t].neutral = Some(index),
                    Some(bottom) => {
                        let top = self.last_stacked(bottom);
                        self.neutral_states[top as usize].next_stacked = Some(index);
                        return;
                    }
                }
            }
        }
    }

    fn last_stacked(&self, mut index: u32) -> u32 {
        while let Some(next) = self.neutral_states[index as usize].next_stacked {
            index = next;
        }
        index
    }

    /// Splits the mini tiles and the tiles into what never changes and what
    /// changes, and starts the map: the areas without their choke points yet.
    fn finish(self) -> Map {
        let (walk_size, size) = (self.walk_size, self.size);
        let mini_tiles = &self.mini_tiles;
        let tiles = &self.tiles;
        let mini_tile_altitudes =
            Grid::from_fn(walk_size.x, walk_size.y, |w| mini_tiles[w].altitude());
        let mini_tile_areas = Grid::from_fn(walk_size.x, walk_size.y, |w| mini_tiles[w].area_id());
        let terrain = Grid::from_fn(size.x, size.y, |t| TileTerrain {
            min_altitude: tiles[t].min_altitude,
            buildable: tiles[t].buildable,
            ground_height: tiles[t].ground_height,
            doodad: tiles[t].doodad,
        });
        let tile_states = Grid::from_fn(size.x, size.y, |t| TileState {
            neutral: tiles[t].neutral,
            area_id: tiles[t].area_id,
        });
        let area_count = self.areas.len();

        Map {
            geometry: Geometry {
                size,
                walk_size,
                center: Position::from(size) / 2,
                max_altitude: self.max_altitude,
                mini_tile_altitudes,
                tiles: terrain,
                starting_locations: self.starting_locations,
                raw_frontier: self.raw_frontier,
                neutrals: self.neutrals,
                geysers: self.geysers,
                areas: self
                    .areas
                    .iter()
                    .map(|&(top, altitude, mini_tiles)| StaticArea::new(top, altitude, mini_tiles))
                    .collect(),
                choke_points: Vec::new(),
                choke_point_traversal: Vec::new(),
                bases: Vec::new(),
                base_count: 0,
            },
            state: State {
                mini_tile_areas,
                tiles: tile_states,
                automatic_path_update: false,
                neutrals: self.neutral_states,
                minerals: self.minerals,
                static_buildings: self.static_buildings,
                areas: vec![AreaState::default(); area_count],
                choke_points: Vec::new(),
                choke_point_distances: Vec::new(),
                choke_point_paths: Vec::new(),
                path_back_trace: Vec::new(),
                bases: Vec::new(),
            },
        }
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
            self.neutrals.push(StaticNeutral {
                kind: n.kind,
                unit_id: n.unit_id,
                type_id: n.type_id,
                position: n.pos,
                top_left: n.top_left,
                size: n.size,
                initial_amount: n.initial_amount,
                blocked_areas: Vec::new(),
            });
            self.neutral_states.push(NeutralState {
                next_stacked: None,
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
        if !self.mini_tiles[p].is_sea() {
            return false;
        }
        DIRS4.iter().any(|&(dx, dy)| {
            let next = p + WalkPosition::new(dx, dy);
            self.valid(next) && !self.mini_tiles[next].is_sea()
        })
    }

    /// A walkable mini tile next to a lake or a neutral.
    fn adjoins8_some_lake_or_neutral(&self, p: WalkPosition) -> bool {
        DIRS8.iter().any(|&(dx, dy)| {
            let next = p + WalkPosition::new(dx, dy);
            self.valid(next)
                && (self.tiles[TilePosition::from(next)].neutral.is_some()
                    || self.mini_tiles[next].is_lake())
        })
    }

    fn walkable_free(&self, w: WalkPosition) -> bool {
        self.mini_tiles[w].is_walkable() && self.tiles[TilePosition::from(w)].neutral.is_none()
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
            if self.neutral_states[candidate as usize]
                .next_stacked
                .is_some()
            {
                continue;
            }
            let StaticNeutral { top_left, size, .. } = self.neutrals[candidate as usize];

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
                    next = self.neutral_states[n as usize].next_stacked;
                }

                let walk_size = WalkPosition::from(size);
                for dy in 0..walk_size.y {
                    for dx in 0..walk_size.x {
                        let w = WalkPosition::from(top_left) + WalkPosition::new(dx, dy);
                        if self.mini_tiles[w].is_walkable() {
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
        let temporary_areas = self.compute_temporary_areas(&by_descending_altitude);
        self.create_areas(&temporary_areas);
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

    fn compute_temporary_areas(
        &mut self,
        by_descending_altitude: &[WalkPosition],
    ) -> Vec<TemporaryArea> {
        // Index 0 unused: area ids are > 0.
        let mut temporary_areas = vec![TemporaryArea {
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
                let id = temporary_areas.len() as i16;
                temporary_areas.push(TemporaryArea {
                    valid: true,
                    id,
                    top: pos,
                    highest_altitude: altitude,
                    size: 1,
                });
                self.mini_tiles[pos].set_area_id(id);
            } else if neighboring.1 == 0 {
                // One neighbouring area: joins it.
                temporary_areas[neighboring.0 as usize].size += 1;
                self.mini_tiles[pos].set_area_id(neighboring.0);
            } else {
                // Two neighbouring areas: joins one of them, and they may merge.
                let mut smaller = neighboring.0;
                let mut bigger = neighboring.1;
                if temporary_areas[smaller as usize].size > temporary_areas[bigger as usize].size {
                    core::mem::swap(&mut smaller, &mut bigger);
                }
                let small = &temporary_areas[smaller as usize];
                let big = &temporary_areas[bigger as usize];
                let near_start = self.starting_locations.iter().any(|&start| {
                    distance(TilePosition::from(pos), start + TilePosition::new(2, 1)) <= 3.0
                });
                let merge = small.size < 80
                    || small.highest_altitude < 80
                    || f64::from(altitude) / f64::from(big.highest_altitude) >= 0.90
                    || f64::from(altitude) / f64::from(small.highest_altitude) >= 0.90
                    || near_start;

                if merge {
                    temporary_areas[bigger as usize].size += 1;
                    self.mini_tiles[pos].set_area_id(bigger);

                    let top = temporary_areas[smaller as usize].top;
                    self.replace_area_ids(top, bigger);
                    let absorbed = temporary_areas[smaller as usize].size;
                    temporary_areas[bigger as usize].size += absorbed;
                    temporary_areas[smaller as usize].valid = false;
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
                    temporary_areas[chosen as usize].size += 1;
                    self.mini_tiles[pos].set_area_id(chosen);
                    self.raw_frontier.push((neighboring, pos));
                }
            }
        }

        // Positions between parts of one area are no frontier.
        self.raw_frontier.retain(|&((a, b), _)| a != b);
        temporary_areas
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
    fn create_areas(&mut self, temporary_areas: &[TemporaryArea]) {
        let mut areas_list: Vec<(WalkPosition, i32)> = Vec::new();
        let mut new_area_id: i16 = 1;
        let mut new_tiny_area_id: i16 = -2;

        for temporary in temporary_areas.iter().filter(|t| t.valid) {
            if temporary.size >= detail::AREA_MIN_MINI_TILES {
                if new_area_id != temporary.id {
                    self.replace_area_ids(temporary.top, new_area_id);
                }
                areas_list.push((temporary.top, temporary.size));
                new_area_id += 1;
            } else {
                self.replace_area_ids(temporary.top, new_tiny_area_id);
                new_tiny_area_id -= 1;
            }
        }

        // `Graph::CreateAreas`.
        self.areas = areas_list
            .iter()
            .map(|&(top, mini_tiles)| (top, self.mini_tiles[top].altitude(), mini_tiles))
            .collect();
    }

    fn set_area_id_in_tile(&mut self, t: TilePosition) {
        let mini_tiles = &self.mini_tiles;
        self.tiles[t].area_id =
            area_id_in_tile(t, self.tiles[t].area_id, |w| mini_tiles[w].area_id());
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
}

/// `SetAreaIdInTile`: from `current`, the area of the walkable mini tiles of
/// the tile if they share one, -1 if they do not.
fn area_id_in_tile(
    t: TilePosition,
    mut current: i16,
    mini_tile_area: impl Fn(WalkPosition) -> i16,
) -> i16 {
    for dy in 0..4 {
        for dx in 0..4 {
            let id = mini_tile_area(WalkPosition::from(t) + WalkPosition::new(dx, dy));
            if id != 0 {
                if current == 0 {
                    current = id;
                } else if current != id {
                    return -1;
                }
            }
        }
    }
    current
}

impl Map {
    /// `SetAreaIdInTile`, after a blocking neutral is destroyed.
    pub(crate) fn set_area_id_in_tile(&mut self, t: TilePosition) {
        let mini_tile_areas = &self.state.mini_tile_areas;
        self.state.tiles[t].area_id =
            area_id_in_tile(t, self.state.tiles[t].area_id, |w| mini_tile_areas[w]);
    }

    pub(crate) fn view(&self) -> MapView<'_> {
        MapView {
            geometry: &self.geometry,
            state: &self.state,
        }
    }

    pub(crate) fn mini_tile_at(&self, w: WalkPosition) -> MiniTile {
        self.view().mini_tile_at(w)
    }

    /// The geometry of the map: what never changes after the analysis.
    pub fn geometry(&self) -> &Geometry {
        &self.geometry
    }

    /// Assigns each start location to the base at most 3 tiles away (Chebyshev),
    /// which moves there. False if a start location gets no base.
    /// `FindBasesForStartingLocations`.
    pub fn find_bases_for_starting_locations(&mut self) -> bool {
        let mut at_least_one_failed = false;
        let geometry = &mut self.geometry;
        for i in 0..geometry.starting_locations.len() {
            let location = geometry.starting_locations[i];
            let mut found = false;
            'areas: for area in &geometry.areas {
                for &base in &area.bases {
                    let base = &mut geometry.bases[base as usize];
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
        self.state.automatic_path_update
    }

    pub fn enable_automatic_path_analysis(&mut self) {
        self.state.automatic_path_update = true;
    }

    /// The size in tiles.
    pub fn size(&self) -> TilePosition {
        self.geometry.size
    }

    /// The size in mini tiles.
    pub fn walk_size(&self) -> WalkPosition {
        self.geometry.walk_size
    }

    pub fn center(&self) -> Position {
        self.geometry.center
    }

    /// The greatest altitude of a mini tile.
    pub fn max_altitude(&self) -> Altitude {
        self.geometry.max_altitude
    }

    pub fn base_count(&self) -> usize {
        self.geometry.base_count
    }

    pub fn choke_point_count(&self) -> usize {
        self.geometry.choke_point_traversal.len()
    }

    pub fn tile(&self, t: TilePosition) -> Option<Tile<'_>> {
        self.valid(t).then(|| Tile::new(self.view(), t))
    }

    pub fn mini_tile(&self, w: WalkPosition) -> Option<MiniTile> {
        self.valid(w).then(|| self.mini_tile_at(w))
    }

    /// Every tile, row by row.
    pub fn tiles(&self) -> impl Iterator<Item = Tile<'_>> {
        let (size, map) = (self.geometry.size, self.view());
        (0..size.y)
            .flat_map(move |y| (0..size.x).map(move |x| Tile::new(map, TilePosition::new(x, y))))
    }

    /// Every mini tile, row by row.
    pub fn mini_tiles(&self) -> impl Iterator<Item = MiniTile> + '_ {
        self.geometry
            .mini_tile_altitudes
            .cells()
            .iter()
            .zip(self.state.mini_tile_areas.cells())
            .map(|(&altitude, &area_id)| MiniTile::new(altitude, area_id))
    }

    pub fn valid<P: MapPosition>(&self, p: P) -> bool {
        p.valid_in(self)
    }

    /// The nearest position inside the map.
    pub fn crop<P: MapPosition>(&self, p: P) -> P {
        p.crop_in(self)
    }

    pub fn starting_locations(&self) -> &[TilePosition] {
        &self.geometry.starting_locations
    }

    /// The neutrals not destroyed: minerals, geysers and static buildings, in
    /// the order of the input.
    pub fn neutrals(&self) -> impl Iterator<Item = Neutral<'_>> {
        let map = self.view();
        (0..self.geometry.neutrals.len() as u32)
            .filter(move |&i| map.state.neutrals[i as usize].alive)
            .map(move |i| Neutral::new(map, i))
    }

    /// The neutral, destroyed or not: see [`Neutral::is_alive`].
    pub fn neutral(&self, id: NeutralId) -> Neutral<'_> {
        assert!(
            (id.0 as usize) < self.geometry.neutrals.len(),
            "a neutral of another map"
        );
        Neutral::new(self.view(), id.0)
    }

    /// The neutral of the unit `unit_id`, unless it was destroyed.
    pub fn neutral_of_unit(&self, unit_id: i32) -> Option<Neutral<'_>> {
        self.neutrals().find(|n| n.unit_id() == unit_id)
    }

    /// A static neutral is gone: a mineral leaves the map, its areas and its
    /// bases (`OnMineralDestroyed`), a static building leaves the map
    /// (`OnStaticBuildingDestroyed`). `None` if no live mineral or static
    /// building is of the unit `unit_id`.
    pub fn remove_neutral(&mut self, unit_id: i32) -> Option<Changes> {
        let neutrals = &self.geometry.neutrals;
        let of_unit = |&i: &u32| neutrals[i as usize].unit_id == unit_id;
        let index = if let Some(i) = self.state.minerals.iter().position(of_unit) {
            let mineral = self.state.minerals.swap_remove(i);
            // `~Mineral`, then `~Neutral`.
            for area in 0..self.geometry.areas.len() {
                self.area_on_mineral_destroyed(area, mineral);
            }
            mineral
        } else {
            let i = self.state.static_buildings.iter().position(of_unit)?;
            self.state.static_buildings.swap_remove(i)
        };

        let blocked_before: Vec<bool> = self.state.choke_points.iter().map(|c| c.blocked).collect();
        let topology_changed = self.destroy_neutral(index);
        let unblocked = blocked_before
            .iter()
            .zip(&self.state.choke_points)
            .enumerate()
            .filter(|(_, (before, now))| **before && !now.blocked)
            .map(|(i, _)| ChokePointId(i as u32))
            .collect();
        let StaticNeutral { top_left, size, .. } = self.geometry.neutrals[index as usize];
        Some(Changes {
            neutral: NeutralId(index),
            tiles: TileRect { top_left, size },
            unblocked,
            topology_changed,
        })
    }

    /// `~Neutral`. Whether distances and paths between choke points, groups and
    /// accessible neighbours were computed again.
    fn destroy_neutral(&mut self, index: u32) -> bool {
        self.remove_from_tiles(index);
        let recomputed = !self.geometry.neutrals[index as usize]
            .blocked_areas
            .is_empty()
            && self.on_blocking_neutral_destroyed(index);
        self.state.neutrals[index as usize].alive = false;
        recomputed
    }

    /// `OnBlockingNeutralDestroyed`: once no neutral is left on its tiles, the
    /// mini tiles of the neutral join the first area it separated. Whether the
    /// distances between choke points were computed again.
    fn on_blocking_neutral_destroyed(&mut self, blocking: u32) -> bool {
        let blocked_areas: Vec<i16> = Neutral::new(self.view(), blocking)
            .blocked_areas()
            .map(|a| a.id().get())
            .collect();

        for &area in &blocked_areas {
            for i in 0..self.geometry.areas[area as usize - 1].choke_points.len() {
                let choke_point = self.geometry.areas[area as usize - 1].choke_points[i];
                self.update_choke_point_after_neutral_removal(choke_point, blocking);
            }
        }

        let StaticNeutral { top_left, size, .. } = self.geometry.neutrals[blocking as usize];
        if self.state.tiles[top_left].neutral.is_some() {
            // Some blocking neutrals remain at the same location.
            return false;
        }

        if let Some(&new_id) = blocked_areas.first() {
            let walk_size = WalkPosition::from(size);
            for dy in 0..walk_size.y {
                for dx in 0..walk_size.x {
                    let w = WalkPosition::from(top_left) + WalkPosition::new(dx, dy);
                    // Walkable: in an area, or in none for the neutral.
                    if self.state.mini_tile_areas[w] != 0 {
                        self.state.mini_tile_areas[w] = new_id;
                    }
                }
            }
        }

        for dy in 0..size.y {
            for dx in 0..size.x {
                let t = top_left + TilePosition::new(dx, dy);
                self.state.tiles[t].area_id = 0;
                self.set_area_id_in_tile(t);
            }
        }

        if self.state.automatic_path_update {
            self.compute_choke_point_distance_matrix(&mut Scratch::new(self.geometry.size));
        }
        self.state.automatic_path_update
    }

    pub fn areas(&self) -> impl ExactSizeIterator<Item = Area<'_>> {
        let map = self.view();
        (1..=self.geometry.areas.len() as i16).map(move |id| Area::new(map, id))
    }

    pub fn area(&self, id: AreaId) -> Area<'_> {
        assert!(
            id.get() as usize <= self.geometry.areas.len(),
            "an area of another map"
        );
        Area::new(self.view(), id.get())
    }

    /// The area of a tile or a mini tile. Prefer [`Map::nearest_area`]: lakes
    /// and tiles shared by areas have none.
    pub fn area_at<P: CellPosition>(&self, p: P) -> Option<Area<'_>> {
        if !self.valid(p) {
            return None;
        }
        let id = P::area_id(&self.state, p);
        (id > 0).then(|| Area::new(self.view(), id))
    }

    /// The area of the position, or else of the nearest tile or mini tile that
    /// has one.
    pub fn nearest_area<P: CellPosition>(&self, p: P) -> Option<Area<'_>> {
        let map = self.view();
        map.nearest_area_id(p).map(|id| Area::new(map, id))
    }

    /// The choke points from `a` to `b` through the graph of choke points, and
    /// the approximate ground distance; no choke point when both are in one
    /// area. `None` when `b` is unreachable from `a`.
    pub fn choke_point_path(&self, a: Position, b: Position) -> Option<ChokePointPath<'_>> {
        let (path, length) = self.graph_get_path(a, b);
        Some(ChokePointPath::new(self.view(), path, Some(length?)))
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
            |map, p| find(P::cell(map.geometry, map.state, p), p),
            |map, p| visit(P::cell(map.geometry, map.state, p), p),
            connect8,
        )
    }

    pub(crate) fn bfs<'m, P: CellPosition>(
        &'m self,
        start: P,
        find: impl FnMut(MapView<'m>, P) -> bool,
        visit: impl FnMut(MapView<'m>, P) -> bool,
        connect8: bool,
    ) -> Option<P> {
        self.view().bfs(start, find, visit, connect8)
    }

    /// The positions where two areas meet, with the ids of the two areas, in
    /// descending order of altitude.
    pub fn raw_frontier(&self) -> &[((i16, i16), WalkPosition)] {
        &self.geometry.raw_frontier
    }

    pub fn choke_point(&self, id: ChokePointId) -> ChokePoint<'_> {
        assert!(
            (id.0 as usize) < self.geometry.choke_points.len(),
            "a choke point of another map"
        );
        ChokePoint::new(self.view(), id.0)
    }

    pub fn base(&self, id: BaseId) -> Base<'_> {
        assert!(
            (id.0 as usize) < self.geometry.bases.len(),
            "a base of another map"
        );
        Base::new(self.view(), id.0)
    }

    /// The base whose depot goes at `location`.
    pub fn base_at(&self, location: TilePosition) -> Option<Base<'_>> {
        let map = self.view();
        (0..self.geometry.bases.len() as u32)
            .map(move |i| Base::new(map, i))
            .find(|b| b.location() == location)
    }

    pub(crate) fn choke_point_distance(&self, a: u32, b: u32) -> i32 {
        self.view().choke_point_distance(a, b)
    }

    pub(crate) fn choke_point_path_between(&self, a: u32, b: u32) -> &[u32] {
        self.view().choke_point_path_between(a, b)
    }
}

impl<'m> MapView<'m> {
    pub(crate) fn mini_tile_at(self, w: WalkPosition) -> MiniTile {
        MiniTile::new(
            self.geometry.mini_tile_altitudes[w],
            self.state.mini_tile_areas[w],
        )
    }

    /// The area id of the position, or else of the nearest tile or mini tile
    /// that has one.
    pub(crate) fn nearest_area_id<P: CellPosition>(self, p: P) -> Option<i16> {
        if !p.inside(self.geometry) {
            return None;
        }
        let id = P::area_id(self.state, p);
        if id > 0 {
            return Some(id);
        }
        let found = self.bfs(p, |map, q| P::area_id(map.state, q) > 0, |_, _| true, true)?;
        Some(P::area_id(self.state, found))
    }

    pub(crate) fn bfs<P: CellPosition>(
        self,
        start: P,
        mut find: impl FnMut(MapView<'m>, P) -> bool,
        mut visit: impl FnMut(MapView<'m>, P) -> bool,
        connect8: bool,
    ) -> Option<P> {
        if !start.inside(self.geometry) {
            return None;
        }
        if find(self, start) {
            return Some(start);
        }
        let mut visited = vec![false; P::grid_len(self.geometry)];
        let mut to_visit = VecDeque::new();
        to_visit.push_back(start);
        visited[P::grid_index(self.geometry, start)] = true;

        let directions: &[(i32, i32)] = if connect8 { &DIRS8 } else { &DIRS4 };
        while let Some(current) = to_visit.pop_front() {
            for &(dx, dy) in directions {
                let next = current.offset(dx, dy);
                if next.inside(self.geometry) {
                    if find(self, next) {
                        return Some(next);
                    }
                    let i = P::grid_index(self.geometry, next);
                    if visit(self, next) && !visited[i] {
                        to_visit.push_back(next);
                        visited[i] = true;
                    }
                }
            }
        }
        None
    }

    /// -1 when unreachable.
    pub(crate) fn choke_point_distance(self, a: u32, b: u32) -> i32 {
        self.state.choke_point_distances[a as usize * self.geometry.choke_points.len() + b as usize]
    }

    /// Empty when unreachable.
    pub(crate) fn choke_point_path_between(self, a: u32, b: u32) -> &'m [u32] {
        &self.state.choke_point_paths[a as usize * self.geometry.choke_points.len() + b as usize]
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
