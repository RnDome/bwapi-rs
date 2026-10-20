// Port of BWEM/include/area.h and BWEM/src/area.cpp of BWEM-community
// (https://github.com/N00byEdge/BWEM-community, 9a63141).
// Copyright (c) 2015, 2017, Igor Dimitrijevic. MIT/X11 License, see LICENSE-BWEM.

//! Areas: connected walkable zones between choke points.

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec;
use alloc::vec::Vec;

use bwapi::{TilePosition, WalkPosition};

use super::base::{BaseData, COMMAND_CENTER_SIZE};
use super::ext::{dist_to_rectangle, fit_to_box, include_point, rounded_dist};
use super::map::Scratch;
use super::{
    Altitude, AreaId, Base, ChokePoint, Geyser, Map, Mineral, Neutral, NeutralKind, Node, detail,
};

/// The data of an area; [`Area`] is its view.
#[derive(Clone, Debug)]
pub(crate) struct AreaData {
    pub(crate) group_id: i16,
    pub(crate) top: WalkPosition,
    pub(crate) top_left: TilePosition,
    pub(crate) bottom_right: TilePosition,
    pub(crate) max_altitude: Altitude,
    pub(crate) mini_tiles: i32,
    pub(crate) tiles: i32,
    pub(crate) buildable_tiles: i32,
    pub(crate) high_ground_tiles: i32,
    pub(crate) very_high_ground_tiles: i32,
    /// The neighbouring areas, by id, and the index of the pair in
    /// `Map::cp_matrix`.
    pub(crate) choke_points_by_area: BTreeMap<i16, usize>,
    pub(crate) accessible_neighbours: Vec<i16>,
    pub(crate) choke_points: Vec<u32>,
    pub(crate) minerals: Vec<u32>,
    pub(crate) geysers: Vec<u32>,
    pub(crate) bases: Vec<u32>,
}

impl AreaData {
    pub(crate) fn new(top: WalkPosition, max_altitude: Altitude, mini_tiles: i32) -> Self {
        AreaData {
            group_id: 0,
            top,
            top_left: TilePosition::new(i32::MAX, i32::MAX),
            bottom_right: TilePosition::new(i32::MIN, i32::MIN),
            max_altitude,
            mini_tiles,
            tiles: 0,
            buildable_tiles: 0,
            high_ground_tiles: 0,
            very_high_ground_tiles: 0,
            choke_points_by_area: BTreeMap::new(),
            accessible_neighbours: Vec::new(),
            choke_points: Vec::new(),
            minerals: Vec::new(),
            geysers: Vec::new(),
            bases: Vec::new(),
        }
    }
}

/// An area of the map.
#[derive(Clone, Copy)]
pub struct Area<'m> {
    map: &'m Map,
    id: i16,
}

impl PartialEq for Area<'_> {
    fn eq(&self, other: &Self) -> bool {
        core::ptr::eq(self.map, other.map) && self.id == other.id
    }
}

impl Eq for Area<'_> {}

impl core::fmt::Debug for Area<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("Area").field(&self.id).finish()
    }
}

impl<'m> Area<'m> {
    pub(crate) fn new(map: &'m Map, id: i16) -> Self {
        Area { map, id }
    }

    fn data(self) -> &'m AreaData {
        &self.map.areas[self.id as usize - 1]
    }

    /// 1 and up.
    pub fn id(self) -> AreaId {
        AreaId(self.id)
    }

    /// Areas of one group are reachable from each other.
    pub fn group_id(self) -> i16 {
        self.data().group_id
    }

    /// The bounding box of its tiles.
    pub fn top_left(self) -> TilePosition {
        self.data().top_left
    }

    pub fn bottom_right(self) -> TilePosition {
        self.data().bottom_right
    }

    pub fn bounding_box_size(self) -> TilePosition {
        self.bottom_right() - self.top_left() + TilePosition::new(1, 1)
    }

    /// The mini tile of the highest altitude.
    pub fn top(self) -> WalkPosition {
        self.data().top
    }

    pub fn max_altitude(self) -> Altitude {
        self.data().max_altitude
    }

    /// The number of its mini tiles.
    pub fn mini_tiles(self) -> i32 {
        self.data().mini_tiles
    }

    /// Percentages of its tiles; 0 when it has no tile of its own (BWEM divides
    /// by zero).
    pub fn low_ground_percentage(self) -> i32 {
        let d = self.data();
        percentage(
            d.tiles - d.high_ground_tiles - d.very_high_ground_tiles,
            d.tiles,
        )
    }

    pub fn high_ground_percentage(self) -> i32 {
        percentage(self.data().high_ground_tiles, self.data().tiles)
    }

    pub fn very_high_ground_percentage(self) -> i32 {
        percentage(self.data().very_high_ground_tiles, self.data().tiles)
    }

    /// Its choke points: those to each neighbouring area.
    pub fn choke_points(self) -> impl ExactSizeIterator<Item = ChokePoint<'m>> + 'm {
        let map = self.map;
        self.data()
            .choke_points
            .iter()
            .map(move |&cp| ChokePoint::new(map, cp))
    }

    /// Its choke points to `area`; none if `area` is no neighbour.
    pub fn choke_points_with(
        self,
        area: Area<'_>,
    ) -> impl ExactSizeIterator<Item = ChokePoint<'m>> + 'm {
        let map = self.map;
        let cps: &'m [u32] = match self.data().choke_points_by_area.get(&area.id) {
            Some(&pair) => &map.cp_matrix[pair],
            None => &[],
        };
        cps.iter().map(move |&cp| ChokePoint::new(map, cp))
    }

    /// The neighbouring areas, by id, each with the choke points to it.
    pub fn choke_points_by_area(
        self,
    ) -> impl Iterator<Item = (Area<'m>, impl ExactSizeIterator<Item = ChokePoint<'m>> + 'm)> + 'm
    {
        let map = self.map;
        self.data()
            .choke_points_by_area
            .iter()
            .map(move |(&id, &pair)| {
                let cps = map.cp_matrix[pair]
                    .iter()
                    .map(move |&cp| ChokePoint::new(map, cp));
                (Area::new(map, id), cps)
            })
    }

    /// The neighbouring areas through a choke point that is not blocked.
    pub fn accessible_neighbours(self) -> impl ExactSizeIterator<Item = Area<'m>> + 'm {
        let map = self.map;
        self.data()
            .accessible_neighbours
            .iter()
            .map(move |&id| Area::new(map, id))
    }

    pub fn accessible_from(self, area: Area<'_>) -> bool {
        self.group_id() == area.group_id()
    }

    pub fn minerals(self) -> impl ExactSizeIterator<Item = Mineral<'m>> + 'm {
        let map = self.map;
        self.data()
            .minerals
            .iter()
            .map(move |&i| Mineral(Neutral::new(map, i)))
    }

    pub fn geysers(self) -> impl ExactSizeIterator<Item = Geyser<'m>> + 'm {
        let map = self.map;
        self.data()
            .geysers
            .iter()
            .map(move |&i| Geyser(Neutral::new(map, i)))
    }

    pub fn bases(self) -> impl ExactSizeIterator<Item = Base<'m>> + 'm {
        let map = self.map;
        self.data().bases.iter().map(move |&i| Base::new(map, i))
    }

    pub fn map(self) -> &'m Map {
        self.map
    }
}

fn percentage(part: i32, whole: i32) -> i32 {
    (part * 100).checked_div(whole).unwrap_or(0)
}

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

impl Map {
    /// `Area::ComputeDistances(ChokePoint, ChokePoints)`: from the middle of
    /// `start` to the middle of each target, inside `area`.
    pub(crate) fn area_compute_distances(
        &self,
        s: &mut Scratch,
        area: i16,
        start: u32,
        targets: &[u32],
    ) -> Vec<i32> {
        let in_area = |cp: u32| {
            let pos = ChokePoint::new(self, cp).pos_in_area(Node::Middle, Area::new(self, area));
            let from = TilePosition::from(pos);
            self.bfs(
                from,
                |map, t| map.tiles[t].area_id == area,
                |_, _| true,
                true,
            )
            .unwrap_or(from)
        };
        let start = in_area(start);
        let targets: Vec<TilePosition> = targets.iter().map(|&cp| in_area(cp)).collect();
        self.tile_distances(s, area, start, &targets)
    }

    /// `Area::ComputeDistances(TilePosition, TilePositions)`: Dijkstra over the
    /// tiles of `area` and the tiles shared by areas; distances in pixels, 0 for
    /// unreachable targets.
    fn tile_distances(
        &self,
        s: &mut Scratch,
        area: i16,
        start: TilePosition,
        targets: &[TilePosition],
    ) -> Vec<i32> {
        let mut distances = vec![0; targets.len()];
        s.unmark_all();

        let mut seq = 0u64;
        let mut to_visit = BTreeSet::new();
        to_visit.insert((0, seq, start));
        s.seq[start] = seq;
        seq += 1;

        let mut remaining_targets = targets.len() as isize;
        while let Some((current_dist, _, current)) = to_visit.pop_first() {
            s.internal[current] = 0;
            s.set_marked(current);

            for (i, &target) in targets.iter().enumerate() {
                if current == target {
                    distances[i] = (0.5 + f64::from(current_dist * 32) / 10000.0) as i32;
                    remaining_targets -= 1;
                }
            }
            if remaining_targets == 0 {
                break;
            }

            for (dx, dy) in DIRS8 {
                let diagonal_move = dx != 0 && dy != 0;
                let new_next_dist = current_dist + if diagonal_move { 14142 } else { 10000 };

                let next = current + TilePosition::new(dx, dy);
                if !self.valid(next) || s.marked(next) {
                    continue;
                }
                let old_dist = s.internal[next];
                if old_dist != 0 {
                    // Already in the queue.
                    if new_next_dist < old_dist {
                        to_visit.remove(&(old_dist, s.seq[next], next));
                        s.internal[next] = new_next_dist;
                        to_visit.insert((new_next_dist, seq, next));
                        s.seq[next] = seq;
                        seq += 1;
                    }
                } else if self.tiles[next].area_id == area || self.tiles[next].area_id == -1 {
                    s.internal[next] = new_next_dist;
                    to_visit.insert((new_next_dist, seq, next));
                    s.seq[next] = seq;
                    seq += 1;
                }
            }
        }

        for &(_, _, t) in &to_visit {
            s.internal[t] = 0;
        }
        distances
    }

    /// `Area::UpdateAccessibleNeighbours`.
    pub(crate) fn update_accessible_neighbours(&mut self, area: usize) {
        let neighbours: Vec<i16> = self.areas[area]
            .choke_points_by_area
            .iter()
            .filter(|&(_, &pair)| {
                self.cp_matrix[pair]
                    .iter()
                    .any(|&cp| !self.choke_points[cp as usize].blocked)
            })
            .map(|(&id, _)| id)
            .collect();
        self.areas[area].accessible_neighbours = neighbours;
    }

    /// `Area::AddTileInformation`, for each tile of the area.
    pub(crate) fn add_tile_information(&mut self, area: usize, t: TilePosition) {
        let tile = self.tiles[t];
        let a = &mut self.areas[area];
        a.tiles += 1;
        if tile.buildable {
            a.buildable_tiles += 1;
        }
        if tile.ground_height == 1 {
            a.high_ground_tiles += 1;
        }
        if tile.ground_height == 2 {
            a.very_high_ground_tiles += 1;
        }
        include_point(&mut a.top_left, &mut a.bottom_right, t);
    }

    /// `Area::OnMineralDestroyed`: also for the bases of an area the mineral is
    /// not in.
    pub(crate) fn area_on_mineral_destroyed(&mut self, area: usize, mineral: u32) {
        let a = &mut self.areas[area];
        if let Some(i) = a.minerals.iter().position(|&m| m == mineral) {
            a.minerals.swap_remove(i);
        }
        for &base in &self.areas[area].bases {
            self.bases[base as usize].on_mineral_destroyed(mineral);
        }
    }

    /// The score of a depot at `location`: the sum of the potential fields under
    /// it, or -1 where a depot cannot stand. `ComputeBaseLocationScore`.
    fn compute_base_location_score(&self, s: &Scratch, area: i16, location: TilePosition) -> i32 {
        let mut sum_score = 0;
        for dy in 0..COMMAND_CENTER_SIZE.y {
            for dx in 0..COMMAND_CENTER_SIZE.x {
                let t = location + TilePosition::new(dx, dy);
                if !self.valid(t) {
                    return -1;
                }
                let tile = &self.tiles[t];
                if !tile.buildable {
                    return -1;
                }
                // A resource at most 3 tiles away, which Starcraft forbids.
                if s.internal[t] == -1 {
                    return -1;
                }
                if tile.area_id != area {
                    return -1;
                }
                if let Some(n) = tile.neutral
                    && self.neutrals[n as usize].kind == NeutralKind::StaticBuilding
                {
                    return -1;
                }
                sum_score += s.internal[t];
            }
        }
        sum_score
    }

    /// No geyser and no mineral but small ones (at most 8, reported in
    /// `blocking_minerals`) within 3 tiles, and far enough from the bases of the
    /// area. `ValidateBaseLocation`.
    fn validate_base_location(
        &self,
        area: i16,
        location: TilePosition,
        blocking_minerals: &mut Vec<u32>,
    ) -> bool {
        blocking_minerals.clear();

        for dy in -3..COMMAND_CENTER_SIZE.y + 3 {
            for dx in -3..COMMAND_CENTER_SIZE.x + 3 {
                let t = location + TilePosition::new(dx, dy);
                if !self.valid(t) {
                    continue;
                }
                if let Some(n) = self.tiles[t].neutral {
                    let neutral = &self.neutrals[n as usize];
                    match neutral.kind {
                        NeutralKind::Geyser => return false,
                        NeutralKind::Mineral if neutral.initial_amount <= 8 => {
                            blocking_minerals.push(n);
                        }
                        NeutralKind::Mineral => return false,
                        NeutralKind::StaticBuilding | NeutralKind::Other => {}
                    }
                }
            }
        }

        self.areas[area as usize - 1].bases.iter().all(|&base| {
            rounded_dist(self.bases[base as usize].location, location)
                >= detail::MIN_TILES_BETWEEN_BASES
        })
    }

    /// Repeatedly finds the best depot location for the remaining resources of
    /// the area, with potential fields, and assigns it the resources near it.
    /// `Area::CreateBases`.
    pub(crate) fn create_bases_in_area(&mut self, s: &mut Scratch, area: i16) {
        const MAX_DIST: i32 = detail::MAX_TILES_BETWEEN_COMMAND_CENTER_AND_RESOURCES;
        let dim_cc = COMMAND_CENTER_SIZE;
        let index = area as usize - 1;

        let mut remaining: Vec<u32> = Vec::new();
        for &m in &self.areas[index].minerals {
            let n = &self.neutrals[m as usize];
            if n.initial_amount >= 40 && n.blocked_areas.is_empty() {
                remaining.push(m);
            }
        }
        for &g in &self.areas[index].geysers {
            let n = &self.neutrals[g as usize];
            if n.initial_amount >= 300 && n.blocked_areas.is_empty() {
                remaining.push(g);
            }
        }

        while !remaining.is_empty() {
            // 1) The search box: no need to search far from the resources.
            let mut top_left_resources = TilePosition::new(i32::MAX, i32::MAX);
            let mut bottom_right_resources = TilePosition::new(i32::MIN, i32::MIN);
            for &r in &remaining {
                let n = &self.neutrals[r as usize];
                include_point(
                    &mut top_left_resources,
                    &mut bottom_right_resources,
                    n.top_left,
                );
                let bottom_right = n.top_left + n.size - TilePosition::new(1, 1);
                include_point(
                    &mut top_left_resources,
                    &mut bottom_right_resources,
                    bottom_right,
                );
            }
            let (area_top_left, area_bottom_right) =
                (self.areas[index].top_left, self.areas[index].bottom_right);
            let fit_bottom_right = area_bottom_right - dim_cc + TilePosition::new(1, 1);
            let mut top_left_search =
                top_left_resources - dim_cc - TilePosition::new(MAX_DIST, MAX_DIST);
            let mut bottom_right_search = bottom_right_resources
                + TilePosition::new(1, 1)
                + TilePosition::new(MAX_DIST, MAX_DIST);
            fit_to_box(&mut top_left_search, area_top_left, fit_bottom_right);
            fit_to_box(&mut bottom_right_search, area_top_left, fit_bottom_right);

            // 2) Potential fields: the closer to each resource, the higher.
            for &r in &remaining {
                let n = &self.neutrals[r as usize];
                for dy in -dim_cc.y - MAX_DIST..n.size.y + dim_cc.y + MAX_DIST {
                    for dx in -dim_cc.x - MAX_DIST..n.size.x + dim_cc.x + MAX_DIST {
                        let t = n.top_left + TilePosition::new(dx, dy);
                        if self.valid(t) {
                            let dist =
                                (dist_to_rectangle(t.center(), n.top_left, n.size) + 16) / 32;
                            let mut score = (MAX_DIST + 3 - dist).max(0);
                            if n.kind == NeutralKind::Geyser {
                                // Somewhat compensates a lone geyser against minerals.
                                score *= 3;
                            }
                            if self.tiles[t].area_id == area {
                                s.internal[t] += score;
                            }
                        }
                    }
                }
            }

            // 3) No depot within 3 tiles of a resource.
            for &r in &remaining {
                let n = &self.neutrals[r as usize];
                for dy in -3..n.size.y + 3 {
                    for dx in -3..n.size.x + 3 {
                        let t = n.top_left + TilePosition::new(dx, dy);
                        if self.valid(t) {
                            s.internal[t] = -1;
                        }
                    }
                }
            }

            // 4) The best location in the search box.
            let mut best_location = TilePosition::new(0, 0);
            let mut best_score = 0;
            let mut blocking_minerals = Vec::new();
            for y in top_left_search.y..=bottom_right_search.y {
                for x in top_left_search.x..=bottom_right_search.x {
                    let location = TilePosition::new(x, y);
                    let score = self.compute_base_location_score(s, area, location);
                    if score > best_score
                        && self.validate_base_location(area, location, &mut blocking_minerals)
                    {
                        best_score = score;
                        best_location = location;
                    }
                }
            }

            // 5) Clear the potential fields.
            for &r in &remaining {
                let n = &self.neutrals[r as usize];
                for dy in -dim_cc.y - MAX_DIST..n.size.y + dim_cc.y + MAX_DIST {
                    for dx in -dim_cc.x - MAX_DIST..n.size.x + dim_cc.x + MAX_DIST {
                        let t = n.top_left + TilePosition::new(dx, dy);
                        if self.valid(t) {
                            s.internal[t] = 0;
                        }
                    }
                }
            }

            if best_score == 0 {
                break;
            }

            // 6) A base at the best location, with the resources near it.
            let assigned: Vec<u32> = remaining
                .iter()
                .copied()
                .filter(|&r| {
                    dist_to_rectangle(self.neutrals[r as usize].pos, best_location, dim_cc) + 2
                        <= MAX_DIST * 32
                })
                .collect();
            remaining.retain(|r| !assigned.contains(r));

            if assigned.is_empty() {
                break;
            }

            let (minerals, geysers): (Vec<u32>, Vec<u32>) = assigned
                .iter()
                .partition(|&&r| self.neutrals[r as usize].kind == NeutralKind::Mineral);
            let base = self.bases.len() as u32;
            self.bases.push(BaseData::new(
                area,
                best_location,
                minerals,
                geysers,
                blocking_minerals,
            ));
            self.areas[index].bases.push(base);
        }
    }
}
