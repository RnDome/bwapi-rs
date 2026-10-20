// Port of BWEB (https://github.com/Cmccrave/BWEB, v1.17, d57980c), PathFind.cpp.
// Copyright (c) 2017 Christian McCrave. MIT License, see LICENSE-BWEB.

use alloc::collections::BTreeSet;
use alloc::vec;
use alloc::vec::Vec;

use bwapi::{Position, TilePosition};

use super::{Footprint, Layout, jps};
use crate::PositionExt;
use crate::stdsort;

/// The moves of A*, in the order BWEB tries them.
const EIGHT_DIR: [TilePosition; 8] = [
    TilePosition::new(0, 1),
    TilePosition::new(1, 0),
    TilePosition::new(-1, 0),
    TilePosition::new(0, -1),
    TilePosition::new(-1, -1),
    TilePosition::new(-1, 1),
    TilePosition::new(1, -1),
    TilePosition::new(1, 1),
];

/// `TilePositions::Invalid`.
const INVALID_TILE: TilePosition = TilePosition::new(1000, 1000);

/// The A* data of a tile: `TileData`.
#[derive(Clone, Copy, Debug, Default)]
struct TileData {
    parent: TilePosition,
    g: f64,
    last_visited_id: i32,
    is_closed: bool,
}

/// What BWEB keeps in globals between searches: the end points of searches
/// that found no path, and the tiles of A*. One for the paths of a map.
///
/// BWEB's cache of unreachable tiles is kept as it is: a search is skipped if
/// its source and its target both failed before, whatever the walkability it
/// was searched with. [`Pathfinding::clear_cache_fully`] forgets them; BWEB
/// calls it when a building appears or goes.
#[derive(Clone, Debug)]
pub struct Pathfinding {
    size: TilePosition,
    not_reachable: BTreeSet<TilePosition>,
    tile_data: Vec<TileData>,
    current_id: i32,
}

impl Pathfinding {
    pub fn new(layout: &Layout) -> Pathfinding {
        let size = layout.size();
        Pathfinding {
            size,
            not_reachable: BTreeSet::new(),
            tile_data: vec![TileData::default(); (size.x * size.y).max(0) as usize],
            current_id: 0,
        }
    }

    /// `Pathfinding::clearCacheFully`: forgets the searches that found no path.
    pub fn clear_cache_fully(&mut self) {
        self.not_reachable.clear();
    }

    fn expect_reachable(&self, source: TilePosition, target: TilePosition) -> bool {
        !self.not_reachable.contains(&source) || !self.not_reachable.contains(&target)
    }

    fn valid(&self, t: TilePosition) -> bool {
        t.x >= 0 && t.y >= 0 && t.x < self.size.x && t.y < self.size.y
    }

    fn one_dim(&self, t: TilePosition) -> usize {
        (t.y * self.size.x + t.x) as usize
    }
}

/// A path of tiles from a source to a target: `BWEB::Path`.
#[derive(Clone, Debug)]
pub struct Path {
    tiles: Vec<TilePosition>,
    dist: f64,
    reachable: bool,
    reverse: bool,
    source: TilePosition,
    target: TilePosition,
    footprint: Footprint,
}

impl Default for Path {
    /// `Path()`: from and to `TilePositions::Invalid`, for `UnitTypes::None`.
    fn default() -> Path {
        Path::new(INVALID_TILE, INVALID_TILE, Footprint::NONE)
    }
}

impl PartialEq for Path {
    /// Paths with the same source and target are equal.
    fn eq(&self, other: &Path) -> bool {
        self.source == other.source && self.target == other.target
    }
}

impl Path {
    /// A path not searched yet for a unit of the type. BWEB's `diagonal` and
    /// `cached` are not used in v1.17.
    pub fn new(
        source: impl Into<TilePosition>,
        target: impl Into<TilePosition>,
        footprint: Footprint,
    ) -> Path {
        Path {
            tiles: Vec::new(),
            dist: 0.0,
            reachable: false,
            reverse: false,
            source: source.into(),
            target: target.into(),
            footprint,
        }
    }

    /// The tiles of the path from the source to the target, or the other way
    /// with [`Path::set_reverse`]. From A*, every tile; from JPS, the source and
    /// the turns.
    pub fn tiles(&self) -> &[TilePosition] {
        &self.tiles
    }

    pub fn source(&self) -> TilePosition {
        self.source
    }

    pub fn target(&self) -> TilePosition {
        self.target
    }

    /// The length in pixels from JPS; 0 from A*, which BWEB does not measure.
    pub fn distance(&self) -> f64 {
        self.dist
    }

    pub fn is_reachable(&self) -> bool {
        self.reachable
    }

    pub fn set_reverse(&mut self, reverse: bool) {
        self.reverse = reverse;
    }

    /// Whether the unit walks through the tile, buildings aside.
    pub fn terrain_walkable(&self, layout: &Layout, tile: TilePosition) -> bool {
        layout.is_walkable(tile, &self.footprint)
    }

    /// Whether the unit walks through the tile and no building is on it; the
    /// source always.
    pub fn unit_walkable(&self, layout: &Layout, tile: TilePosition) -> bool {
        tile == self.source
            || (layout.is_walkable(tile, &self.footprint) && layout.is_used(tile, 1, 1).is_none())
    }

    /// `generateAS`: A* from the target to the source over the tiles
    /// `walkable` lets through, 32 pixels a straight step and 45.248 a diagonal
    /// one plus `cost` of the tile entered (BWEB calls it the heuristic). A
    /// diagonal step needs both tiles beside it walkable. The closures get the
    /// path, as BWEB's capture it.
    pub fn generate_as(
        &mut self,
        pathfinding: &mut Pathfinding,
        mut cost: impl FnMut(&Path, TilePosition) -> f64,
        mut walkable: impl FnMut(&Path, TilePosition) -> bool,
    ) {
        let (source, target) = (self.source, self.target);
        if !pathfinding.valid(source)
            || !pathfinding.valid(target)
            || !pathfinding.expect_reachable(source, target)
        {
            return;
        }

        pathfinding.current_id += 1;
        if pathfinding.current_id == i32::MAX {
            pathfinding.current_id = 1;
            pathfinding.tile_data.fill(TileData::default());
        }
        let id = pathfinding.current_id;

        self.dist = 0.0;
        self.reachable = false;
        self.tiles.clear();

        // `std::priority_queue` with `std::greater`: the least `f` on top.
        let mut open: Vec<(TilePosition, f64)> = Vec::new();
        let greater = |a: &(TilePosition, f64), b: &(TilePosition, f64)| a.1 > b.1;

        let start = pathfinding.one_dim(target);
        pathfinding.tile_data[start] = TileData {
            parent: target,
            g: 0.0,
            last_visited_id: id,
            is_closed: false,
        };
        open.push((target, f64::from(source.approx_distance(target))));

        let found = loop {
            if open.is_empty() {
                break false;
            }
            stdsort::pop_heap(&mut open, greater);
            let (tile, _) = open.pop().unwrap();
            let current = pathfinding.one_dim(tile);

            let data = pathfinding.tile_data[current];
            if data.is_closed && data.last_visited_id == id {
                continue;
            }
            if tile == source {
                break true;
            }
            pathfinding.tile_data[current].is_closed = true;

            for d in EIGHT_DIR {
                let next = tile + d;
                if !pathfinding.valid(next) {
                    continue;
                }
                let diagonal = d.x != 0 && d.y != 0;
                let move_cost = if diagonal { 45.248 } else { 32.0 } + cost(self, next);
                let current_g = pathfinding.tile_data[current].g + move_cost;

                if !walkable(self, next) {
                    continue;
                }
                if diagonal
                    && (!walkable(self, TilePosition::new(tile.x + d.x, tile.y))
                        || !walkable(self, TilePosition::new(tile.x, tile.y + d.y)))
                {
                    continue;
                }

                let index = pathfinding.one_dim(next);
                let next_data = &mut pathfinding.tile_data[index];
                if next_data.last_visited_id != id || current_g < next_data.g {
                    *next_data = TileData {
                        parent: tile,
                        g: current_g,
                        last_visited_id: id,
                        is_closed: false,
                    };
                    open.push((next, current_g + f64::from(source.approx_distance(next))));
                    stdsort::push_heap(&mut open, greater);
                }
            }
        };

        if found {
            self.reachable = true;
            let mut step = source;
            while step != target {
                self.tiles.push(step);
                step = pathfinding.tile_data[pathfinding.one_dim(step)].parent;
            }
            self.tiles.push(target);
            if self.reverse {
                self.tiles.reverse();
            }
        } else {
            pathfinding.not_reachable.insert(source);
            pathfinding.not_reachable.insert(target);
        }
    }

    /// `generateAS_h`: A* with a cost and every tile walkable.
    pub fn generate_as_h(
        &mut self,
        pathfinding: &mut Pathfinding,
        cost: impl FnMut(&Path, TilePosition) -> f64,
    ) {
        self.generate_as(pathfinding, cost, |_, _| true);
    }

    /// `generateAS_w`: A* without a cost.
    pub fn generate_as_w(
        &mut self,
        pathfinding: &mut Pathfinding,
        walkable: impl FnMut(&Path, TilePosition) -> bool,
    ) {
        self.generate_as(pathfinding, |_, _| 0.0, walkable);
    }

    /// `generateJPS`: Jump Point Search over the tiles `walkable` lets through
    /// (the source and the target always). Unlike [`Path::generate_as`], a
    /// diagonal step needs only one of the two tiles beside it walkable, so a
    /// path may cut a corner A* would not; the path is not always the shortest
    /// one. The tiles
    /// are the source and the turns; the distance sums the straight lines
    /// between them, from the top left corners of the tiles.
    pub fn generate_jps(
        &mut self,
        pathfinding: &mut Pathfinding,
        mut walkable: impl FnMut(&Path, TilePosition) -> bool,
    ) {
        self.dist = 0.0;
        self.reachable = false;
        self.tiles.clear();
        let (source, target) = (self.source, self.target);
        if !pathfinding.expect_reachable(source, target) {
            return;
        }

        // BWEB lets a cell one past the last column or row through to its
        // walkability function, which has nothing there.
        let size = pathfinding.size;
        let this = &*self;
        let grid = |x: i32, y: i32| {
            let tile = TilePosition::new(x, y);
            if x > size.x || y > size.y || x < 0 || y < 0 {
                false
            } else {
                tile == source || tile == target || walkable(this, tile)
            }
        };
        let found = jps::find_path(grid, source, target);

        match found {
            Some(turns) => {
                let mut current = Position::from(source);
                self.tiles.push(source);
                for t in turns {
                    self.dist += Position::from(t).distance(current);
                    current = Position::from(t);
                    self.tiles.push(t);
                }
                self.reachable = true;
                if self.reverse {
                    self.tiles.reverse();
                }
            }
            None => {
                pathfinding.not_reachable.insert(source);
                pathfinding.not_reachable.insert(target);
            }
        }
    }
}
