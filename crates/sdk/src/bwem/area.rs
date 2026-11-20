// Port of BWEM/include/area.h and BWEM/src/area.cpp of BWEM-community
// (https://github.com/N00byEdge/BWEM-community, 9a63141).
// Copyright (c) 2015, 2017, Igor Dimitrijevic. MIT/X11 License, see LICENSE-BWEM.

//! Areas: connected walkable zones between choke points.

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec;
use alloc::vec::Vec;

use bwapi::{TilePosition, WalkPosition};

use super::base::{BaseState, COMMAND_CENTER_SIZE, StaticBase};
use super::geometry_helpers::{distance_to_rectangle, fit_to_box, include_point, rounded_distance};
use super::map::{MapView, Scratch};
use super::{
    Altitude, AreaId, Base, ChokePoint, Map, Neutral, NeutralId, NeutralKind, Node, detail,
};

/// What never changes in an area after the analysis; [`Area`] is its view.
#[derive(Clone, Debug)]
pub(crate) struct StaticArea {
    pub(crate) top: WalkPosition,
    pub(crate) top_left: TilePosition,
    pub(crate) bottom_right: TilePosition,
    pub(crate) max_altitude: Altitude,
    pub(crate) mini_tiles: i32,
    pub(crate) tiles: i32,
    pub(crate) buildable_tiles: i32,
    pub(crate) high_ground_tiles: i32,
    pub(crate) very_high_ground_tiles: i32,
    /// The neighbouring areas, by id, each with the choke points to it.
    pub(crate) choke_points_by_area: BTreeMap<i16, Vec<u32>>,
    pub(crate) choke_points: Vec<u32>,
    pub(crate) geysers: Vec<u32>,
    pub(crate) bases: Vec<u32>,
    /// Its minerals as of the analysis.
    pub(crate) initial_minerals: Vec<NeutralId>,
}

/// What changes in an area when neutrals are destroyed.
#[derive(Clone, Debug, Default)]
pub(crate) struct AreaState {
    pub(crate) group_id: i16,
    pub(crate) accessible_neighbours: Vec<i16>,
    /// Its minerals not mined out.
    pub(crate) minerals: Vec<u32>,
}

impl StaticArea {
    pub(crate) fn new(top: WalkPosition, max_altitude: Altitude, mini_tiles: i32) -> Self {
        StaticArea {
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
            choke_points: Vec::new(),
            geysers: Vec::new(),
            bases: Vec::new(),
            initial_minerals: Vec::new(),
        }
    }
}

/// The shares of the tiles of an area by ground height, in percent; 0 when it
/// has no tile of its own (BWEM divides by zero).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GroundHeights {
    pub low: i32,
    pub high: i32,
    pub very_high: i32,
}

/// An area of the map.
#[derive(Clone, Copy)]
pub struct Area<'m> {
    map: MapView<'m>,
    id: i16,
}

impl PartialEq for Area<'_> {
    fn eq(&self, other: &Self) -> bool {
        core::ptr::eq(self.map.geometry, other.map.geometry) && self.id == other.id
    }
}

impl Eq for Area<'_> {}

impl core::fmt::Debug for Area<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("Area").field(&self.id).finish()
    }
}

impl<'m> Area<'m> {
    pub(crate) fn new(map: MapView<'m>, id: i16) -> Self {
        Area { map, id }
    }

    fn data(self) -> &'m StaticArea {
        &self.map.geometry.areas[self.id as usize - 1]
    }

    fn state(self) -> &'m AreaState {
        &self.map.state.areas[self.id as usize - 1]
    }

    /// 1 and up.
    pub fn id(self) -> AreaId {
        AreaId(self.id)
    }

    /// Areas of one group are reachable from each other.
    pub fn group(self) -> i16 {
        self.state().group_id
    }

    /// The top left and the bottom right tiles of the box around its tiles, as
    /// of the analysis.
    pub fn bounds(self) -> (TilePosition, TilePosition) {
        (self.data().top_left, self.data().bottom_right)
    }

    /// The mini tile of the highest altitude.
    pub fn top(self) -> WalkPosition {
        self.data().top
    }

    pub fn max_altitude(self) -> Altitude {
        self.data().max_altitude
    }

    /// The number of its mini tiles, as of the analysis.
    pub fn size_in_mini_tiles(self) -> i32 {
        self.data().mini_tiles
    }

    pub fn ground_heights(self) -> GroundHeights {
        let d = self.data();
        GroundHeights {
            low: percentage(
                d.tiles - d.high_ground_tiles - d.very_high_ground_tiles,
                d.tiles,
            ),
            high: percentage(d.high_ground_tiles, d.tiles),
            very_high: percentage(d.very_high_ground_tiles, d.tiles),
        }
    }

    /// Its choke points: those to each neighbouring area.
    pub fn choke_points(self) -> impl ExactSizeIterator<Item = ChokePoint<'m>> + 'm {
        let map = self.map;
        self.data()
            .choke_points
            .iter()
            .map(move |&i| ChokePoint::new(map, i))
    }

    /// Its choke points to `other`; none if `other` is no neighbour.
    pub fn choke_points_to(
        self,
        other: AreaId,
    ) -> impl ExactSizeIterator<Item = ChokePoint<'m>> + 'm {
        let map = self.map;
        let choke_points: &'m [u32] = self
            .data()
            .choke_points_by_area
            .get(&other.get())
            .map_or(&[], Vec::as_slice);
        choke_points.iter().map(move |&i| ChokePoint::new(map, i))
    }

    /// The neighbouring areas, through any choke point, by id.
    pub fn neighbours(self) -> impl ExactSizeIterator<Item = Area<'m>> + 'm {
        let map = self.map;
        self.data()
            .choke_points_by_area
            .keys()
            .map(move |&id| Area::new(map, id))
    }

    /// The neighbouring areas through a choke point that is not blocked.
    pub fn accessible_neighbours(self) -> impl ExactSizeIterator<Item = Area<'m>> + 'm {
        let map = self.map;
        self.state()
            .accessible_neighbours
            .iter()
            .map(move |&id| Area::new(map, id))
    }

    pub fn is_reachable_from(self, other: AreaId) -> bool {
        self.group() == Area::new(self.map, other.get()).group()
    }

    /// Its minerals not mined out, in the order BWEM keeps them.
    pub fn minerals(self) -> impl ExactSizeIterator<Item = Neutral<'m>> + 'm {
        let map = self.map;
        self.state()
            .minerals
            .iter()
            .map(move |&i| Neutral::new(map, i))
    }

    /// Its minerals as of the analysis.
    pub fn initial_minerals(self) -> &'m [NeutralId] {
        &self.data().initial_minerals
    }

    pub fn geysers(self) -> impl ExactSizeIterator<Item = Neutral<'m>> + 'm {
        let map = self.map;
        self.data()
            .geysers
            .iter()
            .map(move |&i| Neutral::new(map, i))
    }

    pub fn bases(self) -> impl ExactSizeIterator<Item = Base<'m>> + 'm {
        let map = self.map;
        self.data().bases.iter().map(move |&i| Base::new(map, i))
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
        scratch: &mut Scratch,
        area: i16,
        start: u32,
        targets: &[u32],
    ) -> Vec<i32> {
        let in_area = |choke_point: u32| {
            let node =
                ChokePoint::new(self.view(), choke_point).node_in_area(Node::Middle, AreaId(area));
            let from = TilePosition::from(node);
            self.bfs(
                from,
                |map, t| map.state.tiles[t].area_id == area,
                |_, _| true,
                true,
            )
            .unwrap_or(from)
        };
        let start = in_area(start);
        let targets: Vec<TilePosition> = targets.iter().map(|&i| in_area(i)).collect();
        self.tile_distances(scratch, area, start, &targets)
    }

    /// `Area::ComputeDistances(TilePosition, TilePositions)`: Dijkstra over the
    /// tiles of `area` and the tiles shared by areas; distances in pixels, 0 for
    /// unreachable targets.
    fn tile_distances(
        &self,
        scratch: &mut Scratch,
        area: i16,
        start: TilePosition,
        targets: &[TilePosition],
    ) -> Vec<i32> {
        let mut distances = vec![0; targets.len()];
        scratch.unmark_all();

        let mut order = 0u64;
        let mut to_visit = BTreeSet::new();
        to_visit.insert((0, order, start));
        scratch.insertion_order[start] = order;
        order += 1;

        let mut remaining_targets = targets.len() as isize;
        while let Some((current_distance, _, current)) = to_visit.pop_first() {
            scratch.internal[current] = 0;
            scratch.set_marked(current);

            for (i, &target) in targets.iter().enumerate() {
                if current == target {
                    distances[i] = (0.5 + f64::from(current_distance * 32) / 10000.0) as i32;
                    remaining_targets -= 1;
                }
            }
            if remaining_targets == 0 {
                break;
            }

            for (dx, dy) in DIRS8 {
                let diagonal_move = dx != 0 && dy != 0;
                let new_next_distance =
                    current_distance + if diagonal_move { 14142 } else { 10000 };

                let next = current + TilePosition::new(dx, dy);
                if !self.valid(next) || scratch.marked(next) {
                    continue;
                }
                let old_distance = scratch.internal[next];
                if old_distance != 0 {
                    // Already in the queue.
                    if new_next_distance < old_distance {
                        to_visit.remove(&(old_distance, scratch.insertion_order[next], next));
                        scratch.internal[next] = new_next_distance;
                        to_visit.insert((new_next_distance, order, next));
                        scratch.insertion_order[next] = order;
                        order += 1;
                    }
                } else if self.state.tiles[next].area_id == area
                    || self.state.tiles[next].area_id == -1
                {
                    scratch.internal[next] = new_next_distance;
                    to_visit.insert((new_next_distance, order, next));
                    scratch.insertion_order[next] = order;
                    order += 1;
                }
            }
        }

        for &(_, _, t) in &to_visit {
            scratch.internal[t] = 0;
        }
        distances
    }

    /// `Area::UpdateAccessibleNeighbours`.
    pub(crate) fn update_accessible_neighbours(&mut self, area: usize) {
        let neighbours: Vec<i16> = self.geometry.areas[area]
            .choke_points_by_area
            .iter()
            .filter(|(_, choke_points)| {
                choke_points
                    .iter()
                    .any(|&i| !self.state.choke_points[i as usize].blocked)
            })
            .map(|(&id, _)| id)
            .collect();
        self.state.areas[area].accessible_neighbours = neighbours;
    }

    /// `Area::AddTileInformation`, for each tile of the area.
    pub(crate) fn add_tile_information(&mut self, area: usize, t: TilePosition) {
        let tile = self.geometry.tiles[t];
        let a = &mut self.geometry.areas[area];
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
        let a = &mut self.state.areas[area];
        if let Some(i) = a.minerals.iter().position(|&m| m == mineral) {
            a.minerals.swap_remove(i);
        }
        for &base in &self.geometry.areas[area].bases {
            self.state.bases[base as usize].on_mineral_destroyed(mineral);
        }
    }

    /// The score of a depot at `location`: the sum of the potential fields under
    /// it, or -1 where a depot cannot stand. `ComputeBaseLocationScore`.
    fn compute_base_location_score(
        &self,
        scratch: &Scratch,
        area: i16,
        location: TilePosition,
    ) -> i32 {
        let mut sum_score = 0;
        for dy in 0..COMMAND_CENTER_SIZE.y {
            for dx in 0..COMMAND_CENTER_SIZE.x {
                let t = location + TilePosition::new(dx, dy);
                if !self.valid(t) {
                    return -1;
                }
                let tile = &self.state.tiles[t];
                if !self.geometry.tiles[t].buildable {
                    return -1;
                }
                // A resource at most 3 tiles away, which Starcraft forbids.
                if scratch.internal[t] == -1 {
                    return -1;
                }
                if tile.area_id != area {
                    return -1;
                }
                if let Some(n) = tile.neutral
                    && self.geometry.neutrals[n as usize].kind == NeutralKind::StaticBuilding
                {
                    return -1;
                }
                sum_score += scratch.internal[t];
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
                if let Some(n) = self.state.tiles[t].neutral {
                    let neutral = &self.geometry.neutrals[n as usize];
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

        self.geometry.areas[area as usize - 1]
            .bases
            .iter()
            .all(|&base| {
                rounded_distance(self.geometry.bases[base as usize].location, location)
                    >= detail::MIN_TILES_BETWEEN_BASES
            })
    }

    /// Repeatedly finds the best depot location for the remaining resources of
    /// the area, with potential fields, and assigns it the resources near it.
    /// `Area::CreateBases`.
    pub(crate) fn create_bases_in_area(&mut self, scratch: &mut Scratch, area: i16) {
        const MAX_DISTANCE: i32 = detail::MAX_TILES_BETWEEN_COMMAND_CENTER_AND_RESOURCES;
        let depot_size = COMMAND_CENTER_SIZE;
        let index = area as usize - 1;

        let mut remaining: Vec<u32> = Vec::new();
        for &m in &self.state.areas[index].minerals {
            let n = &self.geometry.neutrals[m as usize];
            if n.initial_amount >= 40 && n.blocked_areas.is_empty() {
                remaining.push(m);
            }
        }
        for &g in &self.geometry.areas[index].geysers {
            let n = &self.geometry.neutrals[g as usize];
            if n.initial_amount >= 300 && n.blocked_areas.is_empty() {
                remaining.push(g);
            }
        }

        while !remaining.is_empty() {
            // 1) The search box: no need to search far from the resources.
            let mut top_left_resources = TilePosition::new(i32::MAX, i32::MAX);
            let mut bottom_right_resources = TilePosition::new(i32::MIN, i32::MIN);
            for &r in &remaining {
                let n = &self.geometry.neutrals[r as usize];
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
            let (area_top_left, area_bottom_right) = (
                self.geometry.areas[index].top_left,
                self.geometry.areas[index].bottom_right,
            );
            let fit_bottom_right = area_bottom_right - depot_size + TilePosition::new(1, 1);
            let mut top_left_search =
                top_left_resources - depot_size - TilePosition::new(MAX_DISTANCE, MAX_DISTANCE);
            let mut bottom_right_search = bottom_right_resources
                + TilePosition::new(1, 1)
                + TilePosition::new(MAX_DISTANCE, MAX_DISTANCE);
            fit_to_box(&mut top_left_search, area_top_left, fit_bottom_right);
            fit_to_box(&mut bottom_right_search, area_top_left, fit_bottom_right);

            // 2) Potential fields: the closer to each resource, the higher.
            for &r in &remaining {
                let n = &self.geometry.neutrals[r as usize];
                for dy in -depot_size.y - MAX_DISTANCE..n.size.y + depot_size.y + MAX_DISTANCE {
                    for dx in -depot_size.x - MAX_DISTANCE..n.size.x + depot_size.x + MAX_DISTANCE {
                        let t = n.top_left + TilePosition::new(dx, dy);
                        if self.valid(t) {
                            let distance =
                                (distance_to_rectangle(t.center(), n.top_left, n.size) + 16) / 32;
                            let mut score = (MAX_DISTANCE + 3 - distance).max(0);
                            if n.kind == NeutralKind::Geyser {
                                // Somewhat compensates a lone geyser against minerals.
                                score *= 3;
                            }
                            if self.state.tiles[t].area_id == area {
                                scratch.internal[t] += score;
                            }
                        }
                    }
                }
            }

            // 3) No depot within 3 tiles of a resource.
            for &r in &remaining {
                let n = &self.geometry.neutrals[r as usize];
                for dy in -3..n.size.y + 3 {
                    for dx in -3..n.size.x + 3 {
                        let t = n.top_left + TilePosition::new(dx, dy);
                        if self.valid(t) {
                            scratch.internal[t] = -1;
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
                    let score = self.compute_base_location_score(scratch, area, location);
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
                let n = &self.geometry.neutrals[r as usize];
                for dy in -depot_size.y - MAX_DISTANCE..n.size.y + depot_size.y + MAX_DISTANCE {
                    for dx in -depot_size.x - MAX_DISTANCE..n.size.x + depot_size.x + MAX_DISTANCE {
                        let t = n.top_left + TilePosition::new(dx, dy);
                        if self.valid(t) {
                            scratch.internal[t] = 0;
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
                    distance_to_rectangle(
                        self.geometry.neutrals[r as usize].position,
                        best_location,
                        depot_size,
                    ) + 2
                        <= MAX_DISTANCE * 32
                })
                .collect();
            remaining.retain(|r| !assigned.contains(r));

            if assigned.is_empty() {
                break;
            }

            let (minerals, geysers): (Vec<u32>, Vec<u32>) = assigned
                .iter()
                .partition(|&&r| self.geometry.neutrals[r as usize].kind == NeutralKind::Mineral);
            let base = self.geometry.bases.len() as u32;
            self.geometry
                .bases
                .push(StaticBase::new(area, best_location, geysers));
            self.state.bases.push(BaseState {
                minerals,
                blocking_minerals,
            });
            self.geometry.areas[index].bases.push(base);
        }
    }
}
