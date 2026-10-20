// Port of BWEM/include/graph.h, BWEM/src/graph.cpp and the constructor of
// ChokePoint in BWEM/src/cp.cpp of BWEM-community
// (https://github.com/N00byEdge/BWEM-community, 9a63141).
// Copyright (c) 2015, 2017, Igor Dimitrijevic. MIT/X11 License, see LICENSE-BWEM.

//! The graph of areas and choke points: `BWEM::detail::Graph`.

use alloc::collections::{BTreeMap, BTreeSet, VecDeque};
use alloc::vec;
use alloc::vec::Vec;

use bwapi::{Position, TilePosition, WalkPosition};

use super::cp::{ChokePointState, StaticChokePoint};
use super::geometry_helpers::intersect;
use super::map::Scratch;
use super::{ChokePoint, Map, Neutral, Node, detail};

/// The choke points between each pair of areas, by the pair (smaller id
/// first), in the order they are made.
type ChokePointsByPair = BTreeMap<(i16, i16), Vec<u32>>;

fn pair(a: i16, b: i16) -> (i16, i16) {
    if a < b { (a, b) } else { (b, a) }
}

/// Where the distances between choke points are computed: `ComputeChokePointDistances<Context>`.
#[derive(Clone, Copy)]
enum Context {
    /// Over the tiles of one area.
    Area(i16),
    /// Over the graph of choke points.
    Graph,
}

impl Map {
    /// One choke point per cluster of the frontier between each pair of areas,
    /// and one pseudo choke point per pair of areas each blocking neutral
    /// separates. `Graph::CreateChokePoints`.
    pub(crate) fn create_choke_points(&mut self) {
        let blocking_neutrals: Vec<u32> = self
            .state
            .static_buildings
            .iter()
            .chain(&self.state.minerals)
            .copied()
            .filter(|&n| !self.geometry.neutrals[n as usize].blocked_areas.is_empty())
            .collect();

        // 1) The choke points of each pair of areas, made below.
        let mut by_pair = ChokePointsByPair::new();

        // 2) The raw frontier of each pair of areas, in its order.
        let mut raw_frontier_by_pair: BTreeMap<(i16, i16), Vec<WalkPosition>> = BTreeMap::new();
        for &((a, b), w) in &self.geometry.raw_frontier {
            raw_frontier_by_pair.entry(pair(a, b)).or_default().push(w);
        }

        // 3) Each frontier, from its highest altitude down, in clusters.
        let cluster_min_distance = (crate::math::sqrt(detail::LAKE_MAX_MINI_TILES as f64)) as i32;
        for ((a, b), frontier) in raw_frontier_by_pair {
            let mut clusters: Vec<VecDeque<WalkPosition>> = Vec::new();
            for w in frontier {
                let mut added = false;
                for cluster in &mut clusters {
                    let (Some(&front), Some(&back)) = (cluster.front(), cluster.back()) else {
                        continue;
                    };
                    let distance_to_front = front.chebyshev(w);
                    let distance_to_back = back.chebyshev(w);
                    if distance_to_front.min(distance_to_back) <= cluster_min_distance {
                        if distance_to_front < distance_to_back {
                            cluster.push_front(w);
                        } else {
                            cluster.push_back(w);
                        }
                        added = true;
                        break;
                    }
                }
                if !added {
                    clusters.push(VecDeque::from([w]));
                }
            }

            for cluster in clusters {
                let choke_point = self.new_choke_point(a, b, cluster.into_iter().collect(), None);
                self.push_choke_point(&mut by_pair, a, b, choke_point);
            }
        }

        // 4) A pseudo choke point for each pair of areas a blocking neutral
        //    separates; with stacked neutrals, for the top one.
        for neutral in blocking_neutrals {
            if self.state.neutrals[neutral as usize].next_stacked.is_some() {
                continue;
            }
            let blocked_areas: Vec<i16> = Neutral::new(self.view(), neutral)
                .blocked_areas()
                .map(|a| a.id().get())
                .collect();
            for &pa in &blocked_areas {
                for &pb in &blocked_areas {
                    if pb == pa {
                        break;
                    }
                    let start =
                        WalkPosition::from(self.geometry.neutrals[neutral as usize].position);
                    let center = self
                        .bfs(
                            start,
                            |map, w| map.mini_tile_at(w).is_walkable(),
                            |_, _| true,
                            true,
                        )
                        .unwrap_or(start);
                    let choke_point = self.new_choke_point(pa, pb, vec![center], Some(neutral));
                    self.push_choke_point(&mut by_pair, pa, pb, choke_point);
                }
            }
        }

        // 5) The choke points of each area, and the list of all of them.
        let n = self.geometry.areas.len();
        for a in 1..=n as i16 {
            for b in 1..a {
                let Some(choke_points) = by_pair.get(&pair(a, b)) else {
                    continue;
                };
                self.add_choke_points(a, b, choke_points);
                self.add_choke_points(b, a, choke_points);
                self.geometry.choke_point_traversal.extend(choke_points);
            }
        }
    }

    fn push_choke_point(
        &mut self,
        by_pair: &mut ChokePointsByPair,
        a: i16,
        b: i16,
        choke_point: (StaticChokePoint, ChokePointState),
    ) {
        let index = self.geometry.choke_points.len() as u32;
        self.geometry.choke_points.push(choke_point.0);
        self.state.choke_points.push(choke_point.1);
        by_pair.entry(pair(a, b)).or_default().push(index);
    }

    /// `Area::AddChokePoints`.
    fn add_choke_points(&mut self, area: i16, other: i16, choke_points: &[u32]) {
        let a = &mut self.geometry.areas[area as usize - 1];
        a.choke_points_by_area.insert(other, choke_points.to_vec());
        a.choke_points.extend(choke_points);
    }

    /// The constructor of `ChokePoint`: the ends of the frontier, its middle at
    /// the local maximum of altitude near the middle of the frontier, and the
    /// node positions inside each area.
    fn new_choke_point(
        &self,
        area1: i16,
        area2: i16,
        geometry: Vec<WalkPosition>,
        blocking_neutral: Option<u32>,
    ) -> (StaticChokePoint, ChokePointState) {
        let blocked = blocking_neutral.is_some();
        // With stacked neutrals, the bottom one.
        let blocking_neutral = blocking_neutral
            .and_then(|n| self.state.tiles[self.geometry.neutrals[n as usize].top_left].neutral);

        let altitude = |i: usize| self.geometry.mini_tile_altitudes[geometry[i]];
        let mut i = geometry.len() / 2;
        while i > 0 && altitude(i - 1) > altitude(i) {
            i -= 1;
        }
        while i + 1 < geometry.len() && altitude(i + 1) > altitude(i) {
            i += 1;
        }
        let nodes = [geometry[0], geometry[i], geometry[geometry.len() - 1]];

        let mut nodes_in_area = [(WalkPosition::new(0, 0), WalkPosition::new(0, 0)); 3];
        for (n, &node) in nodes.iter().enumerate() {
            for area in [area1, area2] {
                let found = self
                    .bfs(
                        node,
                        |map, w| {
                            map.state.mini_tile_areas[w] == area
                                && map.state.tiles[TilePosition::from(w)].neutral.is_none()
                        },
                        |map, w| {
                            let mini_tile = map.mini_tile_at(w);
                            mini_tile.area_id() == area
                                || (blocked
                                    && (mini_tile.is_blocked()
                                        || map.state.tiles[TilePosition::from(w)]
                                            .neutral
                                            .is_some()))
                        },
                        true,
                    )
                    .unwrap_or(node);
                if area == area1 {
                    nodes_in_area[n].0 = found;
                } else {
                    nodes_in_area[n].1 = found;
                }
            }
        }

        (
            StaticChokePoint {
                areas: (area1, area2),
                nodes,
                nodes_in_area,
                geometry,
                pseudo: blocked,
            },
            ChokePointState {
                blocked,
                blocking_neutral,
            },
        )
    }

    /// Distances and paths between every two choke points, the accessible
    /// neighbours of the areas and their groups.
    /// `Graph::ComputeChokePointDistanceMatrix`.
    pub(crate) fn compute_choke_point_distance_matrix(&mut self, scratch: &mut Scratch) {
        // 1) Size the matrices.
        let n = self.geometry.choke_points.len();
        self.state.choke_point_distances = vec![-1; n * n];
        self.state.choke_point_paths = vec![Vec::new(); n * n];
        self.state.path_back_trace.resize(n, None);

        // 2) Inside each area.
        for area in 1..=self.geometry.areas.len() as i16 {
            self.compute_choke_point_distances(scratch, Context::Area(area));
        }

        // 3) Through the areas.
        self.compute_choke_point_distances(scratch, Context::Graph);

        for i in 0..self.geometry.choke_point_traversal.len() {
            let choke_point = self.geometry.choke_point_traversal[i];
            self.set_distance(choke_point, choke_point, 0);
            self.set_path(choke_point, choke_point, vec![choke_point]);
        }

        // 4) The accessible neighbours of each area.
        for area in 0..self.geometry.areas.len() {
            self.update_accessible_neighbours(area);
        }

        // 5) The group of each area.
        self.update_group_ids();
    }

    /// `Graph::ComputeChokePointDistances<Context>`: from each choke point of the
    /// context to the ones before it.
    fn compute_choke_point_distances(&mut self, scratch: &mut Scratch, context: Context) {
        let choke_points = match context {
            Context::Area(area) => self.geometry.areas[area as usize - 1].choke_points.clone(),
            Context::Graph => self.geometry.choke_point_traversal.clone(),
        };
        for (k, &start) in choke_points.iter().enumerate() {
            let targets = &choke_points[..k];
            let distances = match context {
                Context::Area(area) => self.area_compute_distances(scratch, area, start, targets),
                Context::Graph => self.graph_compute_distances(scratch, start, targets),
            };

            for (i, &target) in targets.iter().enumerate() {
                let new_distance = distances[i];
                let existing_distance = self.choke_point_distance(start, target);
                if new_distance != 0
                    && (existing_distance == -1 || new_distance < existing_distance)
                {
                    self.set_distance(start, target, new_distance);

                    let mut path = vec![start, target];
                    if let Context::Graph = context {
                        // The intermediate choke points, from the back trace.
                        let mut prev = self.state.path_back_trace[target as usize];
                        let mut steps = 0;
                        while let Some(p) = prev {
                            if p == start || steps > self.geometry.choke_points.len() {
                                break;
                            }
                            path.insert(1, p);
                            prev = self.state.path_back_trace[p as usize];
                            steps += 1;
                        }
                    }
                    self.set_path(start, target, path);
                }
            }
        }
    }

    fn set_distance(&mut self, a: u32, b: u32, value: i32) {
        let n = self.geometry.choke_points.len();
        self.state.choke_point_distances[a as usize * n + b as usize] = value;
        self.state.choke_point_distances[b as usize * n + a as usize] = value;
    }

    fn set_path(&mut self, a: u32, b: u32, path: Vec<u32>) {
        let n = self.geometry.choke_points.len();
        let mut reversed = path.clone();
        reversed.reverse();
        self.state.choke_point_paths[a as usize * n + b as usize] = path;
        self.state.choke_point_paths[b as usize * n + a as usize] = reversed;
    }

    /// `Graph::ComputeDistances`: Dijkstra over the choke points, through areas,
    /// not through blocked choke points; 0 for unreachable targets. The scratch
    /// data is keyed by the tile of each choke point's center, as in C++.
    fn graph_compute_distances(
        &mut self,
        scratch: &mut Scratch,
        start: u32,
        targets: &[u32],
    ) -> Vec<i32> {
        let mut distances = vec![0; targets.len()];
        let center_tile = |map: &Map, choke_point: u32| {
            TilePosition::from(map.geometry.choke_points[choke_point as usize].nodes[1])
        };
        scratch.unmark_all();

        let mut order = 0u64;
        let mut choke_point_order = vec![0u64; self.geometry.choke_points.len()];
        let mut to_visit: BTreeSet<(i32, u64, u32)> = BTreeSet::new();
        to_visit.insert((0, order, start));
        choke_point_order[start as usize] = order;
        order += 1;

        let mut remaining_targets = targets.len() as isize;
        while let Some((current_distance, _, current)) = to_visit.pop_first() {
            let current_tile = center_tile(self, current);
            scratch.internal[current_tile] = 0;
            scratch.set_marked(current_tile);

            for (i, &target) in targets.iter().enumerate() {
                if current == target {
                    distances[i] = current_distance;
                    remaining_targets -= 1;
                }
            }
            if remaining_targets == 0 {
                break;
            }

            if self.state.choke_points[current as usize].blocked && current != start {
                continue;
            }

            let (area1, area2) = self.geometry.choke_points[current as usize].areas;
            for area in [area1, area2] {
                for k in 0..self.geometry.areas[area as usize - 1].choke_points.len() {
                    let next = self.geometry.areas[area as usize - 1].choke_points[k];
                    if next == current {
                        continue;
                    }
                    let new_next_distance =
                        current_distance.wrapping_add(self.choke_point_distance(current, next));
                    let next_tile = center_tile(self, next);
                    if scratch.marked(next_tile) {
                        continue;
                    }
                    let old_distance = scratch.internal[next_tile];
                    if old_distance != 0 {
                        // Already in the queue.
                        if new_next_distance < old_distance {
                            to_visit.remove(&(
                                old_distance,
                                choke_point_order[next as usize],
                                next,
                            ));
                            scratch.internal[next_tile] = new_next_distance;
                            self.state.path_back_trace[next as usize] = Some(current);
                            to_visit.insert((new_next_distance, order, next));
                            choke_point_order[next as usize] = order;
                            order += 1;
                        }
                    } else {
                        scratch.internal[next_tile] = new_next_distance;
                        self.state.path_back_trace[next as usize] = Some(current);
                        to_visit.insert((new_next_distance, order, next));
                        choke_point_order[next as usize] = order;
                        order += 1;
                    }
                }
            }
        }

        for &(_, _, choke_point) in &to_visit {
            scratch.internal[center_tile(self, choke_point)] = 0;
        }
        distances
    }

    /// `Graph::GetPath(Position, Position, int *)`.
    pub(crate) fn graph_get_path(&self, a: Position, b: Position) -> (&[u32], Option<i32>) {
        let (Some(area_a), Some(area_b)) = (
            self.nearest_area(WalkPosition::from(a)),
            self.nearest_area(WalkPosition::from(b)),
        ) else {
            return (&[], None);
        };

        if area_a == area_b {
            return (&[], Some(a.approx_distance(b)));
        }

        if !area_a.is_reachable_from(area_b.id()) {
            return (&[], None);
        }

        let mut min_distance_a_b = i32::MAX;
        let mut best: Option<(ChokePoint<'_>, ChokePoint<'_>)> = None;
        for choke_point_a in area_a.choke_points().filter(|c| !c.is_blocked()) {
            let distance_a_choke_point_a =
                a.approx_distance(Position::from(choke_point_a.center()));
            for choke_point_b in area_b.choke_points().filter(|c| !c.is_blocked()) {
                let distance_b_choke_point_b =
                    b.approx_distance(Position::from(choke_point_b.center()));
                let distance_a_b = distance_a_choke_point_a
                    + distance_b_choke_point_b
                    + self.choke_point_distance(choke_point_a.id().0, choke_point_b.id().0);
                if distance_a_b < min_distance_a_b {
                    min_distance_a_b = distance_a_b;
                    best = Some((choke_point_a, choke_point_b));
                }
            }
        }

        let Some((best_a, best_b)) = best else {
            return (&[], None);
        };
        let path = self.choke_point_path_between(best_a.id().0, best_b.id().0);

        let mut length = min_distance_a_b;
        if path.len() == 1 {
            let choke_point = best_a;
            let end1 = choke_point.node(Node::End1).center();
            let end2 = choke_point.node(Node::End2).center();
            if intersect(a.x, a.y, b.x, b.y, end1.x, end1.y, end2.x, end2.y) {
                length = a.approx_distance(b);
            } else {
                for node in [Node::End1, Node::End2] {
                    let c = choke_point.node(node).center();
                    let distance_a_b = a.approx_distance(c) + b.approx_distance(c);
                    if distance_a_b < length {
                        length = distance_a_b;
                    }
                }
            }
        }

        (path, Some(length))
    }

    /// `Graph::UpdateGroupIds`: areas connected through accessible neighbours
    /// share a group.
    fn update_group_ids(&mut self) {
        let mut next_group_id: i16 = 1;
        let areas = &mut self.state.areas;
        let mut marked = vec![false; areas.len()];

        for start in 0..areas.len() {
            if marked[start] {
                continue;
            }
            let mut to_visit = vec![start];
            while let Some(current) = to_visit.pop() {
                areas[current].group_id = next_group_id;
                for &next in &areas[current].accessible_neighbours {
                    let next = next as usize - 1;
                    if !marked[next] {
                        marked[next] = true;
                        to_visit.push(next);
                    }
                }
            }
            next_group_id += 1;
        }
    }

    /// `mainArea`: the area of greatest id among the areas of the tiles, as the
    /// last key of the C++ `std::map` is.
    fn main_area(&self, top_left: TilePosition, size: TilePosition) -> Option<i16> {
        let mut result = None;
        for dy in 0..size.y {
            for dx in 0..size.x {
                if let Some(area) = self.area_at(top_left + TilePosition::new(dx, dy)) {
                    let id = area.id().get();
                    result = Some(result.map_or(id, |r: i16| r.max(id)));
                }
            }
        }
        result
    }

    /// The minerals and geysers of each area, and the statistics of its tiles.
    /// `Graph::CollectInformation`.
    pub(crate) fn collect_information(&mut self) {
        for i in 0..self.state.minerals.len() {
            let m = self.state.minerals[i];
            let n = &self.geometry.neutrals[m as usize];
            if let Some(area) = self.main_area(n.top_left, n.size) {
                self.state.areas[area as usize - 1].minerals.push(m);
            }
        }
        for i in 0..self.geometry.geysers.len() {
            let g = self.geometry.geysers[i];
            let n = &self.geometry.neutrals[g as usize];
            if let Some(area) = self.main_area(n.top_left, n.size) {
                self.geometry.areas[area as usize - 1].geysers.push(g);
            }
        }

        let size = self.geometry.size;
        for y in 0..size.y {
            for x in 0..size.x {
                let t = TilePosition::new(x, y);
                let area = self.state.tiles[t].area_id;
                if area > 0 {
                    self.add_tile_information(area as usize - 1, t);
                }
            }
        }
    }

    /// `Graph::CreateBases`.
    pub(crate) fn create_bases(&mut self, scratch: &mut Scratch) {
        self.geometry.base_count = 0;
        for area in 1..=self.geometry.areas.len() as i16 {
            self.create_bases_in_area(scratch, area);
            self.geometry.base_count += self.geometry.areas[area as usize - 1].bases.len();
        }
    }
}
