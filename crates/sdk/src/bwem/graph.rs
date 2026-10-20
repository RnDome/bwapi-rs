// Port of BWEM/include/graph.h, BWEM/src/graph.cpp and the constructor of
// ChokePoint in BWEM/src/cp.cpp of BWEM-community
// (https://github.com/N00byEdge/BWEM-community, 9a63141).
// Copyright (c) 2015, 2017, Igor Dimitrijevic. MIT/X11 License, see LICENSE-BWEM.

//! The graph of areas and choke points: `BWEM::detail::Graph`.

use alloc::collections::{BTreeMap, BTreeSet, VecDeque};
use alloc::vec;
use alloc::vec::Vec;

use bwapi::{Position, TilePosition, WalkPosition};

use super::area::AreaData;
use super::cp::ChokePointData;
use super::ext::intersect;
use super::map::{Scratch, pair_index};
use super::{ChokePoint, Map, Neutral, Node, detail};

/// Where the distances between choke points are computed: `ComputeChokePointDistances<Context>`.
#[derive(Clone, Copy)]
enum Context {
    /// Over the tiles of one area.
    Area(i16),
    /// Over the graph of choke points.
    Graph,
}

impl Map {
    /// `Graph::CreateAreas`.
    pub(crate) fn graph_create_areas(&mut self, areas_list: &[(WalkPosition, i32)]) {
        self.areas = areas_list
            .iter()
            .map(|&(top, mini_tiles)| {
                AreaData::new(top, self.mini_tiles[top].altitude(), mini_tiles)
            })
            .collect();
    }

    /// One choke point per cluster of the frontier between each pair of areas,
    /// and one pseudo choke point per pair of areas each blocking neutral
    /// separates. `Graph::CreateChokePoints`.
    pub(crate) fn create_choke_points(&mut self) {
        let blocking_neutrals: Vec<u32> = self
            .static_buildings
            .iter()
            .chain(&self.minerals)
            .copied()
            .filter(|&n| !self.neutrals[n as usize].blocked_areas.is_empty())
            .collect();

        // 1) The triangular matrix of the pairs of areas.
        let n = self.areas.len();
        self.cp_matrix = vec![Vec::new(); n * n.saturating_sub(1) / 2];

        // 2) The raw frontier of each pair of areas, in its order.
        let mut raw_frontier_by_pair: BTreeMap<(i16, i16), Vec<WalkPosition>> = BTreeMap::new();
        for &((a, b), w) in &self.raw_frontier {
            let pair = if a > b { (b, a) } else { (a, b) };
            raw_frontier_by_pair.entry(pair).or_default().push(w);
        }

        // 3) Each frontier, from its highest altitude down, in clusters.
        let cluster_min_dist = (crate::math::sqrt(detail::LAKE_MAX_MINI_TILES as f64)) as i32;
        for ((a, b), frontier) in raw_frontier_by_pair {
            let mut clusters: Vec<VecDeque<WalkPosition>> = Vec::new();
            for w in frontier {
                let mut added = false;
                for cluster in &mut clusters {
                    let (Some(&front), Some(&back)) = (cluster.front(), cluster.back()) else {
                        continue;
                    };
                    let dist_to_front = front.chebyshev(w);
                    let dist_to_back = back.chebyshev(w);
                    if dist_to_front.min(dist_to_back) <= cluster_min_dist {
                        if dist_to_front < dist_to_back {
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
                let cp = self.new_choke_point(a, b, cluster.into_iter().collect(), None);
                self.push_choke_point(a, b, cp);
            }
        }

        // 4) A pseudo choke point for each pair of areas a blocking neutral
        //    separates; with stacked neutrals, for the top one.
        for neutral in blocking_neutrals {
            if self.neutrals[neutral as usize].next_stacked.is_some() {
                continue;
            }
            let blocked_areas: Vec<i16> = Neutral::new(self, neutral)
                .blocked_areas()
                .map(|a| a.id().get())
                .collect();
            for &pa in &blocked_areas {
                for &pb in &blocked_areas {
                    if pb == pa {
                        break;
                    }
                    let start = WalkPosition::from(self.neutrals[neutral as usize].pos);
                    let center = self
                        .bfs(
                            start,
                            |map, w| map.mini_tiles[w].walkable(),
                            |_, _| true,
                            true,
                        )
                        .unwrap_or(start);
                    let cp = self.new_choke_point(pa, pb, vec![center], Some(neutral));
                    self.push_choke_point(pa, pb, cp);
                }
            }
        }

        // 5) The choke points of each area, and the list of all of them.
        for a in 1..=n as i16 {
            for b in 1..a {
                let pair = pair_index(a, b);
                if self.cp_matrix[pair].is_empty() {
                    continue;
                }
                self.add_choke_points(a, b, pair);
                self.add_choke_points(b, a, pair);
                let cps = self.cp_matrix[pair].clone();
                self.choke_point_list.extend(cps);
            }
        }
    }

    fn push_choke_point(&mut self, a: i16, b: i16, cp: ChokePointData) {
        let index = self.choke_points.len() as u32;
        self.choke_points.push(cp);
        self.cp_matrix[pair_index(a, b)].push(index);
    }

    /// `Area::AddChokePoints`.
    fn add_choke_points(&mut self, area: i16, other: i16, pair: usize) {
        let a = &mut self.areas[area as usize - 1];
        a.choke_points_by_area.insert(other, pair);
        a.choke_points.extend(&self.cp_matrix[pair]);
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
    ) -> ChokePointData {
        let blocked = blocking_neutral.is_some();
        // With stacked neutrals, the bottom one.
        let blocking_neutral =
            blocking_neutral.and_then(|n| self.tiles[self.neutrals[n as usize].top_left].neutral);

        let altitude = |i: usize| self.mini_tiles[geometry[i]].altitude();
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
                            map.mini_tiles[w].area_id() == area
                                && map.tiles[TilePosition::from(w)].neutral.is_none()
                        },
                        |map, w| {
                            let mini_tile = map.mini_tiles[w];
                            mini_tile.area_id() == area
                                || (blocked
                                    && (mini_tile.blocked()
                                        || map.tiles[TilePosition::from(w)].neutral.is_some()))
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

        ChokePointData {
            areas: (area1, area2),
            nodes,
            nodes_in_area,
            geometry,
            blocked,
            blocking_neutral,
            pseudo: blocked,
        }
    }

    /// Distances and paths between every two choke points, the accessible
    /// neighbours of the areas and their groups.
    /// `Graph::ComputeChokePointDistanceMatrix`.
    pub(crate) fn compute_choke_point_distance_matrix(&mut self, s: &mut Scratch) {
        // 1) Size the matrices.
        let n = self.choke_points.len();
        self.cp_distances = vec![-1; n * n];
        self.cp_paths = vec![Vec::new(); n * n];
        self.path_back_trace.resize(n, None);

        // 2) Inside each area.
        for area in 1..=self.areas.len() as i16 {
            self.compute_choke_point_distances(s, Context::Area(area));
        }

        // 3) Through the areas.
        self.compute_choke_point_distances(s, Context::Graph);

        for i in 0..self.choke_point_list.len() {
            let cp = self.choke_point_list[i];
            self.set_distance(cp, cp, 0);
            self.set_path(cp, cp, vec![cp]);
        }

        // 4) The accessible neighbours of each area.
        for area in 0..self.areas.len() {
            self.update_accessible_neighbours(area);
        }

        // 5) The group of each area.
        self.update_group_ids();
    }

    /// `Graph::ComputeChokePointDistances<Context>`: from each choke point of the
    /// context to the ones before it.
    fn compute_choke_point_distances(&mut self, s: &mut Scratch, context: Context) {
        let cps = match context {
            Context::Area(area) => self.areas[area as usize - 1].choke_points.clone(),
            Context::Graph => self.choke_point_list.clone(),
        };
        for (k, &start) in cps.iter().enumerate() {
            let targets = &cps[..k];
            let distances = match context {
                Context::Area(area) => self.area_compute_distances(s, area, start, targets),
                Context::Graph => self.graph_compute_distances(s, start, targets),
            };

            for (i, &target) in targets.iter().enumerate() {
                let new_dist = distances[i];
                let existing_dist = self.cp_distance(start, target);
                if new_dist != 0 && (existing_dist == -1 || new_dist < existing_dist) {
                    self.set_distance(start, target, new_dist);

                    let mut path = vec![start, target];
                    if let Context::Graph = context {
                        // The intermediate choke points, from the back trace.
                        let mut prev = self.path_back_trace[target as usize];
                        let mut steps = 0;
                        while let Some(p) = prev {
                            if p == start || steps > self.choke_points.len() {
                                break;
                            }
                            path.insert(1, p);
                            prev = self.path_back_trace[p as usize];
                            steps += 1;
                        }
                    }
                    self.set_path(start, target, path);
                }
            }
        }
    }

    fn set_distance(&mut self, a: u32, b: u32, value: i32) {
        let n = self.choke_points.len();
        self.cp_distances[a as usize * n + b as usize] = value;
        self.cp_distances[b as usize * n + a as usize] = value;
    }

    fn set_path(&mut self, a: u32, b: u32, path: Vec<u32>) {
        let n = self.choke_points.len();
        let mut reversed = path.clone();
        reversed.reverse();
        self.cp_paths[a as usize * n + b as usize] = path;
        self.cp_paths[b as usize * n + a as usize] = reversed;
    }

    /// `Graph::ComputeDistances`: Dijkstra over the choke points, through areas,
    /// not through blocked choke points; 0 for unreachable targets. The scratch
    /// data is keyed by the tile of each choke point's center, as in C++.
    fn graph_compute_distances(
        &mut self,
        s: &mut Scratch,
        start: u32,
        targets: &[u32],
    ) -> Vec<i32> {
        let mut distances = vec![0; targets.len()];
        let center_tile =
            |map: &Map, cp: u32| TilePosition::from(map.choke_points[cp as usize].nodes[1]);
        s.unmark_all();

        let mut seq = 0u64;
        let mut cp_seq = vec![0u64; self.choke_points.len()];
        let mut to_visit: BTreeSet<(i32, u64, u32)> = BTreeSet::new();
        to_visit.insert((0, seq, start));
        cp_seq[start as usize] = seq;
        seq += 1;

        let mut remaining_targets = targets.len() as isize;
        while let Some((current_dist, _, current)) = to_visit.pop_first() {
            let current_tile = center_tile(self, current);
            s.internal[current_tile] = 0;
            s.set_marked(current_tile);

            for (i, &target) in targets.iter().enumerate() {
                if current == target {
                    distances[i] = current_dist;
                    remaining_targets -= 1;
                }
            }
            if remaining_targets == 0 {
                break;
            }

            let data = &self.choke_points[current as usize];
            if data.blocked && current != start {
                continue;
            }

            let (area1, area2) = data.areas;
            for area in [area1, area2] {
                for k in 0..self.areas[area as usize - 1].choke_points.len() {
                    let next = self.areas[area as usize - 1].choke_points[k];
                    if next == current {
                        continue;
                    }
                    let new_next_dist = current_dist.wrapping_add(self.cp_distance(current, next));
                    let next_tile = center_tile(self, next);
                    if s.marked(next_tile) {
                        continue;
                    }
                    let old_dist = s.internal[next_tile];
                    if old_dist != 0 {
                        // Already in the queue.
                        if new_next_dist < old_dist {
                            to_visit.remove(&(old_dist, cp_seq[next as usize], next));
                            s.internal[next_tile] = new_next_dist;
                            self.path_back_trace[next as usize] = Some(current);
                            to_visit.insert((new_next_dist, seq, next));
                            cp_seq[next as usize] = seq;
                            seq += 1;
                        }
                    } else {
                        s.internal[next_tile] = new_next_dist;
                        self.path_back_trace[next as usize] = Some(current);
                        to_visit.insert((new_next_dist, seq, next));
                        cp_seq[next as usize] = seq;
                        seq += 1;
                    }
                }
            }
        }

        for &(_, _, cp) in &to_visit {
            s.internal[center_tile(self, cp)] = 0;
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

        if !area_a.accessible_from(area_b) {
            return (&[], None);
        }

        let mut min_dist_a_b = i32::MAX;
        let mut best: Option<(ChokePoint<'_>, ChokePoint<'_>)> = None;
        for cp_a in area_a.choke_points().filter(|cp| !cp.blocked()) {
            let dist_a_cp_a = a.approx_distance(Position::from(cp_a.center()));
            for cp_b in area_b.choke_points().filter(|cp| !cp.blocked()) {
                let dist_b_cp_b = b.approx_distance(Position::from(cp_b.center()));
                let dist_a_b =
                    dist_a_cp_a + dist_b_cp_b + self.cp_distance(cp_a.id().0, cp_b.id().0);
                if dist_a_b < min_dist_a_b {
                    min_dist_a_b = dist_a_b;
                    best = Some((cp_a, cp_b));
                }
            }
        }

        let Some((best_a, best_b)) = best else {
            return (&[], None);
        };
        let path = self.cp_path(best_a.id().0, best_b.id().0);

        let mut length = min_dist_a_b;
        if path.len() == 1 {
            let cp = best_a;
            let end1 = cp.pos(Node::End1).center();
            let end2 = cp.pos(Node::End2).center();
            if intersect(a.x, a.y, b.x, b.y, end1.x, end1.y, end2.x, end2.y) {
                length = a.approx_distance(b);
            } else {
                for node in [Node::End1, Node::End2] {
                    let c = cp.pos(node).center();
                    let dist_a_b = a.approx_distance(c) + b.approx_distance(c);
                    if dist_a_b < length {
                        length = dist_a_b;
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
        let mut marked = vec![false; self.areas.len()];

        for start in 0..self.areas.len() {
            if marked[start] {
                continue;
            }
            let mut to_visit = vec![start];
            while let Some(current) = to_visit.pop() {
                self.areas[current].group_id = next_group_id;
                for &next in &self.areas[current].accessible_neighbours {
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
                if let Some(area) = self.area(top_left + TilePosition::new(dx, dy)) {
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
        for i in 0..self.minerals.len() {
            let m = self.minerals[i];
            let n = &self.neutrals[m as usize];
            if let Some(area) = self.main_area(n.top_left, n.size) {
                self.areas[area as usize - 1].minerals.push(m);
            }
        }
        for i in 0..self.geysers.len() {
            let g = self.geysers[i];
            let n = &self.neutrals[g as usize];
            if let Some(area) = self.main_area(n.top_left, n.size) {
                self.areas[area as usize - 1].geysers.push(g);
            }
        }

        for y in 0..self.size.y {
            for x in 0..self.size.x {
                let t = TilePosition::new(x, y);
                let area = self.tiles[t].area_id;
                if area > 0 {
                    self.add_tile_information(area as usize - 1, t);
                }
            }
        }
    }

    /// `Graph::CreateBases`.
    pub(crate) fn create_bases(&mut self, s: &mut Scratch) {
        self.base_count = 0;
        for area in 1..=self.areas.len() as i16 {
            self.create_bases_in_area(s, area);
            self.base_count += self.areas[area as usize - 1].bases.len();
        }
    }
}
