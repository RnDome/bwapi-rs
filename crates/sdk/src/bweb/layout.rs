// Port of BWEB (https://github.com/Cmccrave/BWEB, v1.17, d57980c), BWEB.cpp.
// Copyright (c) 2017 Christian McCrave. MIT License, see LICENSE-BWEB.

use alloc::vec::Vec;

use bwapi::{Position, TilePosition, WalkPosition};

use super::Footprint;
use crate::PositionExt;
use crate::bwem::{self, ChokePoint, MapInput, NeutralKind, Node};
use crate::grid::Grid;
use crate::math;

/// `Positions::Invalid`.
const INVALID_POSITION: Position = Position::new(32000, 32000);

/// The tiles used by buildings and reserved for bases and mining, and the
/// walkability of tiles by unit size: the grids of `BWEB::Map`, which is a
/// namespace of functions in BWEB, not a class.
#[derive(Clone, Debug)]
pub struct Layout {
    size: TilePosition,
    /// `Game::is_walkable` of every mini tile.
    walkable: Grid<WalkPosition, bool>,
    /// `Game::is_buildable` of every tile.
    buildable: Grid<TilePosition, bool>,
    reserve: Grid<TilePosition, bool>,
    /// The type id of the building on the tile.
    used: Grid<TilePosition, Option<i32>>,
    walk_large: Grid<TilePosition, bool>,
    walk_medium: Grid<TilePosition, bool>,
    walk_small: Grid<TilePosition, bool>,
    walk_full: Grid<TilePosition, bool>,
    /// The tiles of the geometry of each choke point, by its index, sorted.
    choke_tiles: Vec<Vec<TilePosition>>,
}

impl Layout {
    /// `Map::onStart`: the walkability grids, the tiles of the choke points, and
    /// the tiles of the neutrals, used if they are buildings and reserved if they
    /// do not move. `input` is what `bwem` was analysed from.
    pub fn new(bwem: &bwem::Map, input: &MapInput) -> Layout {
        let size = bwem.size();
        let mut map = Layout {
            size,
            walkable: input.walkable.clone(),
            buildable: input.buildable.clone(),
            reserve: Grid::filled(size.x, size.y, false),
            used: Grid::filled(size.x, size.y, None),
            walk_large: Grid::filled(size.x, size.y, false),
            walk_medium: Grid::filled(size.x, size.y, false),
            walk_small: Grid::filled(size.x, size.y, false),
            walk_full: Grid::filled(size.x, size.y, false),
            choke_tiles: Vec::new(),
        };
        map.initialize_grids(input);
        map.initialize_choke_geo(bwem);
        map.initialize_neutrals(input);
        map
    }

    /// `initalizeGrids`.
    fn initialize_grids(&mut self, input: &MapInput) {
        for y in 0..self.size.y {
            for x in 0..self.size.x {
                let t = TilePosition::new(x, y);
                let corner = WalkPosition::from(t);
                let mut count = 0;
                for dy in 0..4 {
                    for dx in 0..4 {
                        if self.walkable(corner + WalkPosition::new(dx, dy)) {
                            count += 1;
                        }
                    }
                }
                self.walk_full[t] = count >= 14;
            }
        }

        // The tiles of geysers are used and not walkable.
        for n in input
            .neutrals
            .iter()
            .filter(|n| n.kind == NeutralKind::Geyser)
        {
            for y in n.tile_position.y..n.tile_position.y + 2 {
                for x in n.tile_position.x..n.tile_position.x + 4 {
                    let t = TilePosition::new(x, y);
                    if self.contains(t) {
                        self.used[t] = Some(n.type_id);
                        self.walk_full[t] = false;
                    }
                }
            }
        }

        // The tiles of minerals are used.
        for n in input
            .neutrals
            .iter()
            .filter(|n| n.kind == NeutralKind::Mineral)
        {
            for x in n.tile_position.x..n.tile_position.x + 2 {
                let t = TilePosition::new(x, n.tile_position.y);
                if self.contains(t) {
                    self.used[t] = Some(n.type_id);
                }
            }
        }

        for y in 0..self.size.y {
            for x in 0..self.size.x {
                let t = TilePosition::new(x, y);
                if self.walk_full[t] {
                    self.walk_large[t] = self.large_walkable(t);
                    self.walk_medium[t] = self.medium_walkable(t);
                    // Small units need a walkable tile only.
                    self.walk_small[t] = true;
                }
            }
        }
    }

    /// Whether the 4 mini tiles of the column `dx` of the tile are walkable.
    fn column_walkable(&self, w: WalkPosition, dx: i32) -> bool {
        (0..4).all(|dy| self.walkable(w + WalkPosition::new(dx, dy)))
    }

    /// Two walkable columns of mini tiles on one side of the tile and its four
    /// corners.
    fn large_walkable(&self, t: TilePosition) -> bool {
        let w = WalkPosition::from(t);
        let left = self.column_walkable(w, -2) && self.column_walkable(w, -1);
        let right = self.column_walkable(w, 4) && self.column_walkable(w, 5);
        let corners = [(0, 0), (3, 0), (0, 3), (3, 3)]
            .iter()
            .all(|&(dx, dy)| self.walkable(w + WalkPosition::new(dx, dy)));
        corners && (left || right)
    }

    /// Two walkable columns of mini tiles beside the tile: both on the left,
    /// one on each side, or both on the right.
    fn medium_walkable(&self, t: TilePosition) -> bool {
        let w = WalkPosition::from(t);
        let offset1 = self.column_walkable(w, -2);
        let offset2 = self.column_walkable(w, -1);
        let offset3 = self.column_walkable(w, 4);
        let offset4 = self.column_walkable(w, 5);
        (offset2 && (offset1 || offset3)) || (offset3 && offset4)
    }

    /// `initializeChokeGeo`.
    fn initialize_choke_geo(&mut self, bwem: &bwem::Map) {
        self.choke_tiles = Vec::new();
        for area in bwem.areas() {
            for cp in area.choke_points() {
                let index = cp.id().index() as usize;
                if self.choke_tiles.len() <= index {
                    self.choke_tiles.resize(index + 1, Vec::new());
                }
                let tiles = &mut self.choke_tiles[index];
                tiles.extend(cp.geometry().iter().map(|&w| TilePosition::from(w)));
                tiles.sort_unstable();
                tiles.dedup();
            }
        }
    }

    /// `initializeNeutrals`.
    fn initialize_neutrals(&mut self, input: &MapInput) {
        for n in &input.neutrals {
            if n.immobile {
                self.add_reserve(n.tile_position, n.size.x, n.size.y);
            }
            if n.building {
                self.add_used_rect(n.tile_position, n.size.x, n.size.y, n.type_id);
            }
        }
    }

    /// The size of the map in tiles.
    pub fn size(&self) -> TilePosition {
        self.size
    }

    fn contains(&self, t: TilePosition) -> bool {
        t.x >= 0 && t.y >= 0 && t.x < self.size.x && t.y < self.size.y
    }

    /// `Position::isValid`. Not `bwem::Map::valid`, which takes the mini tile of
    /// the position, so that (-1, -1) is valid.
    fn position_valid(&self, p: Position) -> bool {
        p.x >= 0 && p.y >= 0 && p.x < self.size.x * 32 && p.y < self.size.y * 32
    }

    /// `Game::is_walkable`: false outside the map.
    fn walkable(&self, w: WalkPosition) -> bool {
        self.walkable.get(w).copied().unwrap_or(false)
    }

    /// `Game::is_buildable`: false outside the map.
    pub(crate) fn buildable(&self, t: TilePosition) -> bool {
        self.buildable.get(t).copied().unwrap_or(false)
    }

    /// Reserves the tiles of the rectangle that are in the map.
    pub fn add_reserve(&mut self, t: TilePosition, width: i32, height: i32) {
        self.fill_reserve(t, width, height, true);
    }

    /// Frees the tiles of the rectangle, whoever reserved them.
    pub fn remove_reserve(&mut self, t: TilePosition, width: i32, height: i32) {
        self.fill_reserve(t, width, height, false);
    }

    fn fill_reserve(&mut self, t: TilePosition, width: i32, height: i32, value: bool) {
        for x in t.x..t.x + width {
            for y in t.y..t.y + height {
                if let Some(cell) = self.reserve.get_mut(TilePosition::new(x, y)) {
                    *cell = value;
                }
            }
        }
    }

    /// Whether a tile of the rectangle is reserved.
    pub fn is_reserved(&self, here: TilePosition, width: i32, height: i32) -> bool {
        (here.x..here.x + width).any(|x| {
            (here.y..here.y + height)
                .any(|y| self.reserve.get(TilePosition::new(x, y)) == Some(&true))
        })
    }

    /// Marks the tiles of a building of the type as used.
    pub fn add_used(&mut self, t: TilePosition, footprint: &Footprint) {
        self.add_used_rect(
            t,
            footprint.tile_width,
            footprint.tile_height,
            footprint.type_id,
        );
    }

    fn add_used_rect(&mut self, t: TilePosition, width: i32, height: i32, type_id: i32) {
        self.fill_used(t, width, height, Some(type_id));
    }

    /// Frees the tiles of the rectangle.
    pub fn remove_used(&mut self, t: TilePosition, width: i32, height: i32) {
        self.fill_used(t, width, height, None);
    }

    fn fill_used(&mut self, t: TilePosition, width: i32, height: i32, value: Option<i32>) {
        for x in t.x..t.x + width {
            for y in t.y..t.y + height {
                if let Some(cell) = self.used.get_mut(TilePosition::new(x, y)) {
                    *cell = value;
                }
            }
        }
    }

    /// The type id of the first building found on the rectangle, column by
    /// column.
    pub fn is_used(&self, here: TilePosition, width: i32, height: i32) -> Option<i32> {
        (here.x..here.x + width).find_map(|x| {
            (here.y..here.y + height)
                .find_map(|y| self.used.get(TilePosition::new(x, y)).copied().flatten())
        })
    }

    /// Whether a unit of the type walks through the tile. The size of the unit
    /// picks the grid: at least 30 pixels large, at least 20 medium, at least
    /// 10 small, otherwise the tile must be walkable as a whole. False outside
    /// the map.
    pub fn is_walkable(&self, here: TilePosition, footprint: &Footprint) -> bool {
        let grid = if footprint.width >= 30 || footprint.height >= 30 {
            &self.walk_large
        } else if footprint.width >= 20 || footprint.height >= 20 {
            &self.walk_medium
        } else if footprint.width >= 10 || footprint.height >= 10 {
            &self.walk_small
        } else {
            &self.walk_full
        };
        grid.get(here).copied().unwrap_or(false)
    }

    /// Whether a building of the type fits at `location`: every tile in the map,
    /// buildable, with a walkable top left mini tile and not used; for a building
    /// on creep, the row below buildable. A resource depot must also keep away
    /// from the minerals and geysers of `bwem`, as `canBuildHere` requires;
    /// units and creep in the way are not checked here.
    pub fn can_place(
        &self,
        bwem: &bwem::Map,
        footprint: &Footprint,
        location: TilePosition,
    ) -> bool {
        if footprint.requires_creep {
            for x in location.x..location.x + footprint.tile_width {
                if !self.buildable(TilePosition::new(x, location.y + footprint.tile_height)) {
                    return false;
                }
            }
        }

        if footprint.is_resource_depot && !clear_of_resources(bwem, location) {
            return false;
        }

        for x in location.x..location.x + footprint.tile_width {
            for y in location.y..location.y + footprint.tile_height {
                let tile = TilePosition::new(x, y);
                if !self.contains(tile)
                    || !self.buildable(tile)
                    || !self.walkable(WalkPosition::from(tile))
                    || self.is_used(tile, 1, 1).is_some()
                {
                    return false;
                }
            }
        }
        true
    }

    /// The ground distance from `start` to `end` in pixels, estimated through
    /// the choke points of the path: from each choke point to the next by a
    /// straight line. `None` outside the map or with no path (BWEB gives
    /// `DBL_MAX`, or the straight distance when a position is in no area).
    pub fn ground_distance_via_choke_points(
        &self,
        bwem: &bwem::Map,
        start: Position,
        end: Position,
    ) -> Option<f64> {
        if !self.position_valid(start) || !self.position_valid(end) {
            return None;
        }
        if let (Some(a), Some(b)) = (
            bwem.area_at(WalkPosition::from(start)),
            bwem.area_at(WalkPosition::from(end)),
        ) {
            if !a.is_reachable_from(b.id()) {
                return None;
            }
        }

        // No length: no path.
        let path = bwem.choke_point_path(start, end)?;

        let mut dist = 0.0;
        let mut last = start;
        // The node of the choke point nearest to the last position.
        let fast_closest_node = |cp: ChokePoint<'_>, last: Position| {
            let n1 = Position::from(cp.node(Node::End1));
            let n2 = Position::from(cp.node(Node::End2));
            let n3 = Position::from(cp.center());
            let (d1, d2, d3) = (n1.distance(last), n2.distance(last), n3.distance(last));
            if d1 < d2 {
                if d1 < d3 { n1 } else { n3 }
            } else if d2 < d3 {
                n2
            } else {
                n3
            }
        };
        for (i, cp) in path.iter().enumerate() {
            let large = cp.node(Node::End1).distance(cp.node(Node::End2)) > 40.0;
            let next = if i == 0 && !large {
                self.closest_choke_tile(cp, start)
                    .unwrap_or(INVALID_POSITION)
            } else {
                fast_closest_node(cp, last)
            };
            dist += next.distance(last);
            last = next;
        }
        Some(dist + last.distance(end))
    }

    /// The center of the tile of the choke point's geometry nearest to `here`.
    pub fn closest_choke_tile(&self, cp: ChokePoint<'_>, here: Position) -> Option<Position> {
        let tiles = self.choke_tiles.get(cp.id().index() as usize)?;
        let mut best = f64::MAX;
        let mut pos_best = None;
        for &tile in tiles {
            let p = Position::from(tile) + Position::new(16, 16);
            let dist = p.distance(here);
            if dist < best {
                pos_best = Some(p);
                best = dist;
            }
        }
        pos_best
    }
}

/// The resource check of `canBuildHere` for a resource depot at `lt`: no mineral
/// with its top left tile within 4 tiles left, 3 up, 6 right or 5 down, no
/// geyser within 6 left, 4 up, 6 right or 5 down.
fn clear_of_resources(bwem: &bwem::Map, lt: TilePosition) -> bool {
    let near = |tp: TilePosition, left: i32, up: i32| {
        tp.x > lt.x - left && tp.y > lt.y - up && tp.x < lt.x + 7 && tp.y < lt.y + 6
    };
    !bwem.neutrals().any(|n| match n.kind() {
        NeutralKind::Mineral => near(n.top_left(), 5, 4),
        NeutralKind::Geyser => near(n.top_left(), 7, 5),
        NeutralKind::StaticBuilding | NeutralKind::Other => false,
    })
}

/// `Map::getAngle`: the angle from `p1` to `p2` in radians, from 0 to 2π,
/// counterclockwise on the screen, whose y axis points down.
pub(crate) fn angle(p1: Position, p2: Position) -> f64 {
    let dy = f64::from(p2.y - p1.y);
    let dx = f64::from(p2.x - p1.x);
    let angle = math::atan2(-dy, dx);
    if angle < 0.0 {
        angle + 2.0 * math::PI
    } else {
        angle
    }
}

/// The two positions `length` pixels away from the middle of the segment,
/// perpendicular to it.
pub fn perpendicular_line(points: (Position, Position), length: f64) -> (Position, Position) {
    let (n1, n2) = points;
    let dist = n1.distance(n2);
    let dx1 = (f64::from(n2.x - n1.x) * length / dist) as i32;
    let dy1 = (f64::from(n2.y - n1.y) * length / dist) as i32;
    let dx2 = (f64::from(n1.x - n2.x) * length / dist) as i32;
    let dy2 = (f64::from(n1.y - n2.y) * length / dist) as i32;
    let middle = (n1 + n2) / 2;
    (
        Position::new(-dy1, dx1) + middle,
        Position::new(-dy2, dx2) + middle,
    )
}
