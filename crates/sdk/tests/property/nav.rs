//! Properties of the static navigation of BWEB: the walkability grids by
//! size of unit, the searches of `Path` (A* and JPS), ground distances
//! through choke points and closest choke tiles. Threats are not here.
//!
//! The movement models of BWEB, which the searches are checked against: eight
//! directions; a straight step is 32 pixels, a diagonal one 45.248. A* takes a
//! diagonal step only where both tiles beside it are walkable, searches from
//! the target to the source and pays, for every tile it enters, the step and
//! the cost of the tile; the target is its start and needs no walkability.
//! JPS takes a diagonal step where one tile beside it is walkable, so it cuts
//! one corner, and takes the source and the target as walkable.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};

use bwapi_sdk::bwem::NeutralKind;

use super::bweb::{Context, Plan, Search, search_cost};
use super::{Offenders, Violation};

type Tile = (i32, i32);

/// The modes of the searches.
const AS_TERRAIN: i32 = 0;
const AS_COST: i32 = 1;
const JPS_UNIT: i32 = 3;

/// Steps in thousandths of a pixel, so that sums are exact.
const STRAIGHT: u64 = 32_000;
const DIAGONAL: u64 = 45_248;

const EIGHT: [Tile; 8] = [
    (1, 0),
    (-1, 0),
    (0, 1),
    (0, -1),
    (1, 1),
    (1, -1),
    (-1, 1),
    (-1, -1),
];

/// The violations of the navigation of `plan`.
pub fn check(plan: &Plan, cx: &Context<'_>) -> Vec<Violation> {
    let mut out = Vec::new();
    let mut fail = |property: &'static str, subject: String, detail: String| {
        out.push(Violation {
            property: property.to_owned(),
            subject,
            detail,
        });
    };
    let nav = &plan.navigation;
    let size = cx.analysis.size;
    if nav.walkable.len() != 4
        || nav
            .walkable
            .iter()
            .any(|g| g.len() != (size.0 * size.1) as usize)
    {
        fail(
            "walk_grids",
            "grids".into(),
            format!("{} grids", nav.walkable.len()),
        );
        return out;
    }
    check_grids(plan, cx, &mut fail);
    check_searches(plan, cx, &mut fail);
    check_distances(plan, cx, &mut fail);
    out
}

fn check_grids(plan: &Plan, cx: &Context<'_>, fail: &mut impl FnMut(&'static str, String, String)) {
    let nav = &plan.navigation;
    let size = cx.analysis.size;
    let walk_width = size.0 * 4;
    let bwapi = |x: i32, y: i32| {
        (0..walk_width).contains(&x)
            && (0..size.1 * 4).contains(&y)
            && cx.terrain.walkable[(y * walk_width + x) as usize]
    };
    let mut geyser = vec![false; (size.0 * size.1) as usize];
    for n in cx
        .terrain
        .neutrals
        .iter()
        .filter(|n| n.kind == NeutralKind::Geyser)
    {
        for dy in 0..2 {
            for dx in 0..4 {
                let (x, y) = (n.tile.0 + dx, n.tile.1 + dy);
                if (0..size.0).contains(&x) && (0..size.1).contains(&y) {
                    geyser[(y * size.0 + x) as usize] = true;
                }
            }
        }
    }
    let mut w1 = Offenders::new("walk_grids_monotone", "tiles");
    let mut w2 = Offenders::new("bweb_walk_grids", "tiles");
    for ty in 0..size.1 {
        for tx in 0..size.0 {
            let i = (ty * size.0 + tx) as usize;
            let [full, small, medium, large] = [0, 1, 2, 3].map(|k| nav.walkable[k][i]);
            if (large && !medium) || (medium && !small) || (small && !full) {
                w1.add(
                    (tx, ty),
                    i32::from(large) * 4 + i32::from(medium) * 2 + i32::from(small),
                    0,
                    || format!("full {full}, small {small}, medium {medium}, large {large}"),
                );
            }
            // `initalizeGrids`.
            let (wx, wy) = (tx * 4, ty * 4);
            let count = (0..4)
                .flat_map(|dy| (0..4).map(move |dx| (dx, dy)))
                .filter(|&(dx, dy)| bwapi(wx + dx, wy + dy))
                .count();
            let e_full = count >= 14 && !geyser[i];
            let column = |dx: i32| (0..4).all(|dy| bwapi(wx + dx, wy + dy));
            let (o1, o2, o3, o4) = (column(-2), column(-1), column(4), column(5));
            let corners = [(0, 0), (3, 0), (0, 3), (3, 3)]
                .iter()
                .all(|&(dx, dy)| bwapi(wx + dx, wy + dy));
            let e_medium = e_full && ((o2 && (o1 || o3)) || (o3 && o4));
            let e_large = e_full && corners && ((o1 && o2) || (o3 && o4));
            let expected = [e_full, e_full, e_medium, e_large];
            let got = [full, small, medium, large];
            if expected != got {
                let bits = |g: [bool; 4]| {
                    g.iter()
                        .enumerate()
                        .map(|(k, &b)| i32::from(b) << k)
                        .sum::<i32>()
                };
                w2.add((tx, ty), bits(got), bits(expected), || {
                    format!("full, small, medium, large {got:?}, the rules give {expected:?}")
                });
            }
        }
    }
    w1.report(fail);
    w2.report(fail);

    // W3: `unitWalkable` keeps off the tiles of neutral buildings, minerals and
    // geysers, which the layout after the blocks must have used.
    let used = &plan.snapshots[3].used;
    let mut w3 = Offenders::new("unit_walkable_off_neutrals", "tiles");
    for n in &cx.terrain.neutrals {
        let obstacle = n.building || matches!(n.kind, NeutralKind::Mineral | NeutralKind::Geyser);
        if !obstacle {
            continue;
        }
        for dy in 0..n.size.1 {
            for dx in 0..n.size.0 {
                let (x, y) = (n.tile.0 + dx, n.tile.1 + dy);
                if !((0..size.0).contains(&x) && (0..size.1).contains(&y)) {
                    continue;
                }
                let i = (y * size.0 + x) as usize;
                if used[i].is_none() && (0..4).any(|k| nav.walkable[k][i]) {
                    w3.add((x, y), n.unit_id, 0, || {
                        format!("unit walkable under unit {}", n.unit_id)
                    });
                }
            }
        }
    }
    w3.report(fail);
}

/// The grid a search walks: walkability of its class, for mode 3 without
/// used tiles; JPS takes its source and target as walkable.
struct Grid<'a> {
    size: (i32, i32),
    walkable: &'a [bool],
    used: Option<&'a [Option<i32>]>,
    forced: [Tile; 2],
    /// The diagonal rule of JPS: one tile beside the step walkable is enough.
    jps: bool,
}

impl Grid<'_> {
    fn on_map(&self, t: Tile) -> bool {
        (0..self.size.0).contains(&t.0) && (0..self.size.1).contains(&t.1)
    }

    fn walkable(&self, t: Tile) -> bool {
        if self.forced.contains(&t) {
            return self.on_map(t);
        }
        if !self.on_map(t) {
            return false;
        }
        let i = (t.1 * self.size.0 + t.0) as usize;
        self.walkable[i] && self.used.is_none_or(|u| u[i].is_none())
    }

    /// A step from `a` to the next tile `b`: `b` walkable, and for a diagonal
    /// both tiles beside it (A*) or one (JPS).
    fn step(&self, a: Tile, b: Tile) -> bool {
        if !self.walkable(b) {
            return false;
        }
        if a.0 == b.0 || a.1 == b.1 {
            return true;
        }
        let (side_x, side_y) = (self.walkable((b.0, a.1)), self.walkable((a.0, b.1)));
        if self.jps {
            side_x || side_y
        } else {
            side_x && side_y
        }
    }
}

fn step_length(a: Tile, b: Tile) -> u64 {
    if a.0 != b.0 && a.1 != b.1 {
        DIAGONAL
    } else {
        STRAIGHT
    }
}

/// Dijkstra from `start` over the steps of `grid`, entering a tile costing
/// its step plus `cost` of it, in thousandths of a pixel.
fn dijkstra(grid: &Grid<'_>, start: Tile, cost: impl Fn(Tile) -> u64) -> Vec<u64> {
    let (w, h) = grid.size;
    let mut dist = vec![u64::MAX; (w * h) as usize];
    let at = |t: Tile| (t.1 * w + t.0) as usize;
    if !grid.on_map(start) {
        return dist;
    }
    dist[at(start)] = 0;
    let mut heap = BinaryHeap::new();
    heap.push(Reverse((0u64, start)));
    while let Some(Reverse((d, t))) = heap.pop() {
        if d > dist[at(t)] {
            continue;
        }
        for (dx, dy) in EIGHT {
            let n = (t.0 + dx, t.1 + dy);
            if !grid.step(t, n) {
                continue;
            }
            let nd = d + step_length(t, n) + cost(n);
            if nd < dist[at(n)] {
                dist[at(n)] = nd;
                heap.push(Reverse((nd, n)));
            }
        }
    }
    dist
}

/// The cost in thousandths of a pixel of entering a tile in a search.
fn entry_cost(mode: i32, t: Tile) -> u64 {
    if mode == AS_COST {
        (search_cost(t) * 1000.0) as u64
    } else {
        0
    }
}

fn check_searches(
    plan: &Plan,
    cx: &Context<'_>,
    fail: &mut impl FnMut(&'static str, String, String),
) {
    let nav = &plan.navigation;
    let size = cx.analysis.size;
    let used = plan.snapshots[3].used.as_slice();
    let grid = |s: &Search| Grid {
        size,
        walkable: &nav.walkable[s.class],
        used: (s.mode == JPS_UNIT).then_some(used),
        // A* checks every tile it enters, the source too, and starts from the
        // target, which it does not check; JPS takes both as walkable.
        forced: if s.mode >= 2 {
            [s.source, s.target]
        } else {
            [(-1, -1), (-1, -1)]
        },
        jps: s.mode >= 2,
    };
    let mut fields: BTreeMap<(usize, i32, Tile, Tile), Vec<u64>> = BTreeMap::new();
    let mut failed: BTreeSet<Tile> = BTreeSet::new();
    let mut zero = Offenders::new("diag_a_star_distance_zero", "searches");
    let mut longer = Offenders::new("path_shortest", "searches");
    let mut missed = Offenders::new("path_found_when_one_exists", "searches");
    // A search by class, mode and ends, for the hash.
    let key = |s: &Search| {
        (
            s.class as i32 * 10 + s.mode,
            s.source.0 * 1000 + s.source.1,
            s.target.0 * 1000 + s.target.1,
        )
    };
    for s in &nav.searches {
        let subject = format!(
            "class {} mode {} from {},{} to {},{}",
            s.class, s.mode, s.source.0, s.source.1, s.target.0, s.target.1
        );
        let g = grid(s);
        let cached = failed.contains(&s.source) && failed.contains(&s.target);

        // S1.
        if s.reachable == s.tiles.is_empty() {
            fail(
                "path_found_has_tiles",
                subject.clone(),
                format!("{} tiles", s.tiles.len()),
            );
        }
        if s.reachable {
            let ends = (s.tiles.first().copied(), s.tiles.last().copied());
            if ends != (Some(s.source), Some(s.target)) {
                fail("path_ends", subject.clone(), format!("{ends:?}"));
            }
            if s.source == s.target && (s.tiles.len() != 1 || s.distance != 0.0) {
                fail(
                    "path_to_itself",
                    subject.clone(),
                    format!("{:?}, {}", s.tiles, s.distance),
                );
            }
        }

        // S2 and S3: walkable tiles, steps of the movement model; for JPS
        // straight runs between its turns.
        if s.reachable && !s.tiles.is_empty() {
            let mut expanded = vec![s.tiles[0]];
            let mut broken = None;
            for w in s.tiles.windows(2) {
                let (a, b) = (w[0], w[1]);
                let (dx, dy) = (b.0 - a.0, b.1 - a.1);
                let run = dx == 0 || dy == 0 || dx.abs() == dy.abs();
                let n = dx.abs().max(dy.abs());
                if (s.mode < 2 && n != 1) || !run || n == 0 {
                    broken = Some(format!("{a:?} to {b:?}"));
                    break;
                }
                let (sx, sy) = (dx.signum(), dy.signum());
                for k in 1..=n {
                    let next = (a.0 + sx * k, a.1 + sy * k);
                    let prev = *expanded.last().expect("a tile");
                    // In the direction of the search: A* from the target.
                    let ok = if s.mode < 2 {
                        g.step(next, prev)
                    } else {
                        g.step(prev, next)
                    };
                    if !ok {
                        broken = Some(format!("{prev:?} to {next:?}"));
                    }
                    expanded.push(next);
                }
                if broken.is_some() {
                    break;
                }
            }
            if let Some(b) = broken {
                fail("bweb_path_steps", subject.clone(), b);
            }

            // S5: as short as the shortest under the same model.
            let field = fields
                .entry((
                    s.class,
                    s.mode,
                    s.target,
                    if s.mode >= 2 { s.source } else { s.target },
                ))
                .or_insert_with(|| dijkstra(&g, s.target, |t| entry_cost(s.mode, t)));
            let best = field[(s.source.1 * size.0 + s.source.0) as usize];
            let length: u64 = expanded
                .windows(2)
                .map(|w| step_length(w[0], w[1]) + entry_cost(s.mode, w[0]))
                .sum();
            if best != u64::MAX && length != best {
                let (k, from, to) = key(s);
                // Lengths in thousandths of a pixel fit `i32` on maps up to
                // 256 tiles.
                longer.add_keyed((k, from, to, (length - best) as i32), || {
                    format!(
                        "{subject}: {:.3} pixels, the shortest {:.3}",
                        length as f64 / 1000.0,
                        best as f64 / 1000.0
                    )
                });
            }

            // S6, S7: the length BWEB gives.
            if s.mode >= 2 {
                let px = |t: Tile| (f64::from(t.0 * 32), f64::from(t.1 * 32));
                let line = |a: Tile, b: Tile| {
                    let (a, b) = (px(a), px(b));
                    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
                };
                let sum: f64 = s.tiles.windows(2).map(|w| line(w[0], w[1])).sum();
                if (sum - s.distance).abs() > 1e-6 {
                    fail(
                        "bweb_jps_length",
                        subject.clone(),
                        format!("{}, its turns sum {sum}", s.distance),
                    );
                }
                if s.distance + 1e-6 < line(s.source, s.target) {
                    fail(
                        "path_not_shorter_than_line",
                        subject.clone(),
                        format!("{}", s.distance),
                    );
                }
            } else if s.distance == 0.0 && s.source != s.target {
                zero.add(s.source, s.target.0, s.target.1, || subject.clone());
            }
        }

        // S4: none found though one exists under the same model.
        if !s.reachable {
            let field = fields
                .entry((
                    s.class,
                    s.mode,
                    s.target,
                    if s.mode >= 2 { s.source } else { s.target },
                ))
                .or_insert_with(|| dijkstra(&g, s.target, |t| entry_cost(s.mode, t)));
            let best = field[(s.source.1 * size.0 + s.source.0) as usize];
            if best != u64::MAX {
                let (k, from, to) = key(s);
                missed.add_keyed((k, from, to, i32::from(cached)), || {
                    format!(
                        "{subject}: a path of {:.3} pixels exists{}",
                        best as f64 / 1000.0,
                        if cached {
                            "; both ends failed before (cache)"
                        } else {
                            ""
                        }
                    )
                });
            }
            failed.insert(s.source);
            failed.insert(s.target);
        }
    }
    zero.report(fail);
    longer.report(fail);
    missed.report(fail);
}

fn check_distances(
    plan: &Plan,
    cx: &Context<'_>,
    fail: &mut impl FnMut(&'static str, String, String),
) {
    let nav = &plan.navigation;
    let n = nav.points.len();
    if n == 0 {
        return;
    }
    let size = cx.analysis.size;
    let pos = |i: usize| (f64::from(nav.points[i].0), f64::from(nav.points[i].1));
    let line = |a: (f64, f64), b: (f64, f64)| ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt();
    let valid = |p: (i32, i32)| p.0 >= 0 && p.1 >= 0 && p.0 < size.0 * 32 && p.1 < size.1 * 32;
    let choke = |i: i32| cx.analysis.choke_points.iter().find(|c| c.index == i);
    // The tiles of the geometry of a choke point, as BWEB keeps them.
    let tiles_of = |i: i32| -> Vec<Tile> {
        let mut t: Vec<Tile> = choke(i)
            .map(|c| c.geometry.iter().map(|&(x, y)| (x / 4, y / 4)).collect())
            .unwrap_or_default();
        t.sort_unstable();
        t.dedup();
        t
    };
    let closest = |i: i32, p: (f64, f64)| -> Option<(f64, f64)> {
        let mut best: Option<((f64, f64), f64)> = None;
        for (x, y) in tiles_of(i) {
            let c = (f64::from(x * 32 + 16), f64::from(y * 32 + 16));
            let d = line(c, p);
            if best.is_none_or(|b| d < b.1) {
                best = Some((c, d));
            }
        }
        best.map(|b| b.0)
    };

    let mut shorter = Offenders::new("ground_distance_not_shorter_than_line", "pairs");
    let mut reach = Offenders::new("ground_distance_reachability", "pairs");
    let mut no_area = Offenders::new("diag_ground_distance_in_no_area", "pairs");
    let mut formula = Offenders::new("bweb_ground_distance", "pairs");
    for a in 0..n {
        for b in 0..n {
            let k = a * n + b;
            let d = nav.ground_distances[k];
            let straight = line(pos(a), pos(b));
            let (pa, pb) = (nav.points[a], nav.points[b]);
            let bwem_path = &nav.choke_paths[k];
            let pair = (a as i32, b as i32);
            if let Some(d) = d
                && d + 1e-6 < straight
            {
                shorter.add(pair, 0, 0, || {
                    format!("{pa:?} to {pb:?}: {d}, straight {straight}")
                });
            }
            // C1b: no distance exactly where BWEM has no path, or a position is
            // off the map.
            let none_expected = !valid(pa) || !valid(pb) || nav.bwem_lengths[k].is_none();
            match (d, none_expected) {
                (None, false) => reach.add(pair, 0, 1, || {
                    format!("{pa:?} to {pb:?}: none, BWEM has a path")
                }),
                (Some(d), true) if (d - straight).abs() < 1e-6 => no_area.add(pair, 0, 0, || {
                    format!("{pa:?} to {pb:?}: the straight distance {d}")
                }),
                (Some(d), true) => reach.add(pair, 1, 0, || {
                    format!("{pa:?} to {pb:?}: {d}, BWEM has no path")
                }),
                _ => {}
            }
            // C2: from point to point through the choke points of the path: the
            // first, unless wide, by its closest tile, the others by their
            // node nearest the last point.
            if let Some(d) = d
                && !none_expected
            {
                let mut last = pos(a);
                let mut dist = 0.0;
                for (i, &cp) in bwem_path.iter().enumerate() {
                    let Some(c) = choke(cp) else { continue };
                    let node =
                        |k: usize| (f64::from(c.nodes[k].0 * 8), f64::from(c.nodes[k].1 * 8));
                    let ends = (c.nodes[0], c.nodes[2]);
                    let wide =
                        f64::from((ends.0.0 - ends.1.0).pow(2) + (ends.0.1 - ends.1.1).pow(2))
                            .sqrt()
                            > 40.0;
                    let next = if i == 0 && !wide {
                        closest(cp, pos(a)).unwrap_or((32000.0, 32000.0))
                    } else {
                        let (n1, n2, n3) = (node(0), node(2), node(1));
                        let (d1, d2, d3) = (line(n1, last), line(n2, last), line(n3, last));
                        if d1 < d2 {
                            if d1 < d3 { n1 } else { n3 }
                        } else if d2 < d3 {
                            n2
                        } else {
                            n3
                        }
                    };
                    dist += line(next, last);
                    last = next;
                }
                let expected = dist + line(last, pos(b));
                if (expected - d).abs() > 1e-6 {
                    formula.add(pair, 0, 0, || {
                        format!("{pa:?} to {pb:?}: {d}, through the choke points {expected}")
                    });
                }
            }
        }
    }
    shorter.report(fail);
    reach.report(fail);
    no_area.report(fail);
    formula.report(fail);

    // C3: the closest choke tile is the center of a tile of the geometry with
    // none nearer.
    let mut c3 = Offenders::new("closest_choke_tile", "pairs");
    for (cp, tiles) in &nav.closest_choke_tiles {
        for (p, &t) in tiles.iter().enumerate() {
            let expected = closest(*cp, pos(p));
            let ok = match (t, expected) {
                (Some(t), Some(e)) => {
                    let t = (f64::from(t.0), f64::from(t.1));
                    tiles_of(*cp)
                        .iter()
                        .any(|&(x, y)| (f64::from(x * 32 + 16), f64::from(y * 32 + 16)) == t)
                        && (line(t, pos(p)) - line(e, pos(p))).abs() < 1e-6
                }
                (None, None) => true,
                _ => false,
            };
            if !ok {
                c3.add((*cp, p as i32), 0, 0, || {
                    format!(
                        "choke point {cp} from {:?}: {t:?}, nearest {expected:?}",
                        nav.points[p]
                    )
                });
            }
        }
    }
    c3.report(fail);
}
