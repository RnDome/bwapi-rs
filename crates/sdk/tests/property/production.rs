//! Properties of the way out of the production slots of BWEB, the large
//! pieces of its blocks, and the regression of the Barracks of Road War.
//!
//! The way out starts at the tiles beside the building, its ring: candidates
//! for where a unit leaves it, not where the game makes one. A unit moves as
//! A* does, over the walkability grid of its size class. In the minimal
//! scenario the neutrals, the depots of the stations and the building stand;
//! in the stress one every piece of every block too, an upper bound of what
//! is built, as blocks are places to choose from. Reserves stand in the way of
//! nothing: they are buildings to come.

use std::collections::VecDeque;

use super::bweb::{self, Block, Context, Plan, TERRAN};
use super::{Offenders, Violation};

type Tile = (i32, i32);

/// The Barracks of the game of Road War that its worker never reached: the
/// lower large piece of a production block above the mineral line of the main
/// at the left edge.
const ROAD_WAR: &str = "scbw/(2)Road War";
const ROAD_WAR_BLOCK: Tile = (1, 108);
const ROAD_WAR_BARRACKS: Tile = (1, 111);

const LARGE: (i32, i32) = (4, 3);
const DEPOT: (i32, i32) = (4, 3);

/// The size classes of the units that leave: small, then medium and large.
const SMALL: usize = 1;
const SIZED: [usize; 2] = [2, 3];

/// The violations of the ways out of the production slots of `plan`.
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
    if nav.walkable.len() != 4 {
        return out;
    }
    let world = World::new(plan, cx);

    let mut exit = Offenders::new("diag_production_exit", "slots");
    let mut exit_sized = Offenders::new("diag_production_exit_sized", "slots");
    let mut route = Offenders::new("diag_production_route", "slots");
    let mut route_sized = Offenders::new("diag_production_route_sized", "slots");
    for b in &plan.blocks {
        for &slot in &b.large {
            for class in [SMALL].into_iter().chain(SIZED) {
                let (exit, route) = if class == SMALL {
                    (&mut exit, &mut route)
                } else {
                    (&mut exit_sized, &mut route_sized)
                };
                let way = world.way_out(slot, class);
                // E1a.
                if !way.chokes.is_empty() && way.chokes.iter().all(Option::is_none) {
                    exit.add(slot, class as i32, 0, || {
                        format!(
                            "class {class}: no choke point of area {} reached in the minimal scenario",
                            way.area
                        )
                    });
                }
                // E1b.
                if let Some((base, false)) = way.route {
                    route.add(slot, class as i32, 0, || {
                        format!(
                            "class {class}: the depot of base {},{} is not reached in the stress scenario",
                            base.0, base.1
                        )
                    });
                }
            }
        }
    }
    exit.report(&mut fail);
    exit_sized.report(&mut fail);
    route.report(&mut fail);
    route_sized.report(&mut fail);

    if cx.map == ROAD_WAR && plan.race == TERRAN {
        // E3a.
        let block = plan.blocks.iter().position(|b| b.tile == ROAD_WAR_BLOCK);
        let detail = match block {
            Some(i) if plan.blocks[i].large.contains(&ROAD_WAR_BARRACKS) => {
                let way = world.way_out(ROAD_WAR_BARRACKS, SMALL);
                way.chokes
                    .iter()
                    .zip(&way.choke_ids)
                    .map(|(steps, cp)| match steps {
                        Some(n) => format!("choke point {cp} in {n} steps"),
                        None => format!("choke point {cp} not reached"),
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            }
            _ => "no such slot in the plan".to_owned(),
        };
        let (x, y) = ROAD_WAR_BARRACKS;
        fail(
            "diag_road_war_barracks_exit",
            format!("slot {x},{y}"),
            detail,
        );
        // E3b.
        if let Some(i) = block {
            production_reachable(plan, cx, i, size, &mut fail);
        }
    }
    out
}

/// The obstacles of the scenarios and what the ways out are measured to.
struct World<'a> {
    plan: &'a Plan,
    cx: &'a Context<'a>,
    size: (i32, i32),
    /// The neutrals and the depots of the stations.
    minimal: Vec<bool>,
    /// And every piece of every block.
    stress: Vec<bool>,
}

/// Where the way out of a slot leads.
struct WayOut {
    area: i32,
    /// The choke points of the area of the slot and the steps to the nearest
    /// tile of each in the minimal scenario.
    choke_ids: Vec<i32>,
    chokes: Vec<Option<u32>>,
    /// The location of the nearest base of another area reached in the
    /// minimal scenario, and whether the stress one reaches it too.
    route: Option<(Tile, bool)>,
}

impl<'a> World<'a> {
    fn new(plan: &'a Plan, cx: &'a Context<'a>) -> Self {
        let size = cx.analysis.size;
        let mut minimal: Vec<bool> = plan.snapshots[0].used.iter().map(Option::is_some).collect();
        for s in &plan.stations {
            bweb::mark(&mut minimal, size, s.location, DEPOT);
        }
        let mut stress = minimal.clone();
        for b in &plan.blocks {
            for (at, wh) in bweb::block_footprints(plan.race, b) {
                bweb::mark(&mut stress, size, at, wh);
            }
        }
        World {
            plan,
            cx,
            size,
            minimal,
            stress,
        }
    }

    fn on_map(&self, t: Tile) -> bool {
        (0..self.size.0).contains(&t.0) && (0..self.size.1).contains(&t.1)
    }

    fn at(&self, t: Tile) -> usize {
        (t.1 * self.size.0 + t.0) as usize
    }

    /// Whether a unit of `class` stands on `t` with `obstacles` and the
    /// building at `slot`.
    fn free(&self, class: usize, obstacles: &[bool], slot: Tile, t: Tile) -> bool {
        self.on_map(t)
            && self.plan.navigation.walkable[class][self.at(t)]
            && !obstacles[self.at(t)]
            && !inside(slot, LARGE, t)
    }

    fn way_out(&self, slot: Tile, class: usize) -> WayOut {
        let cx = self.cx;
        let area = if self.on_map(slot) {
            cx.analysis.tile_areas[self.at(slot)]
        } else {
            0
        };
        let exits = ring(slot, LARGE);

        let minimal = |t: Tile| self.free(class, &self.minimal, slot, t);
        let from: Vec<Tile> = exits.iter().copied().filter(|&t| minimal(t)).collect();
        let steps = search(self.size, &minimal, &from, false);

        let cps: Vec<_> = cx
            .analysis
            .choke_points
            .iter()
            .filter(|c| area > 0 && (c.areas.0 == area || c.areas.1 == area))
            .collect();
        let chokes = cps
            .iter()
            .map(|c| {
                c.geometry
                    .iter()
                    .map(|&(x, y)| (x / 4, y / 4))
                    .filter(|&t| self.on_map(t))
                    .map(|t| steps[self.at(t)])
                    .min()
                    .filter(|&n| n != u32::MAX)
            })
            .collect();

        // The nearest base of another area, by the steps to the tiles beside
        // its depot.
        let beside = |location: Tile, steps: &[u32]| {
            ring(location, DEPOT)
                .into_iter()
                .filter(|&t| self.on_map(t))
                .map(|t| steps[self.at(t)])
                .min()
                .unwrap_or(u32::MAX)
        };
        let nearest = cx
            .analysis
            .bases
            .iter()
            .filter(|b| b.area != area)
            .map(|b| (beside(b.location, &steps), b.location))
            .filter(|&(n, _)| n != u32::MAX)
            .min_by_key(|&(n, _)| n);
        let route = nearest.map(|(_, location)| {
            let stress = |t: Tile| self.free(class, &self.stress, slot, t);
            let from: Vec<Tile> = exits.iter().copied().filter(|&t| stress(t)).collect();
            let steps = search(self.size, &stress, &from, false);
            (location, beside(location, &steps) != u32::MAX)
        });
        WayOut {
            area,
            choke_ids: cps.iter().map(|c| c.index).collect(),
            chokes,
            route,
        }
    }
}

fn inside(at: Tile, (w, h): (i32, i32), t: Tile) -> bool {
    (at.0..at.0 + w).contains(&t.0) && (at.1..at.1 + h).contains(&t.1)
}

/// The tiles beside a rectangle, by 8 directions.
fn ring(at: Tile, (w, h): (i32, i32)) -> Vec<Tile> {
    let mut tiles = Vec::new();
    for y in at.1 - 1..=at.1 + h {
        for x in at.0 - 1..=at.0 + w {
            if !inside(at, (w, h), (x, y)) {
                tiles.push((x, y));
            }
        }
    }
    tiles
}

/// The steps from the nearest of `from` over the tiles `open` lets through,
/// by 8 directions; a diagonal one needs both tiles beside it open (A*) or
/// one (JPS). `u32::MAX` where none leads.
fn search(size: (i32, i32), open: &dyn Fn(Tile) -> bool, from: &[Tile], jps: bool) -> Vec<u32> {
    let at = |t: Tile| (t.1 * size.0 + t.0) as usize;
    let mut steps = vec![u32::MAX; (size.0 * size.1) as usize];
    let mut queue = VecDeque::new();
    for &t in from {
        if open(t) && steps[at(t)] == u32::MAX {
            steps[at(t)] = 0;
            queue.push_back(t);
        }
    }
    while let Some(t) = queue.pop_front() {
        for dy in -1..=1 {
            for dx in -1..=1 {
                let n = (t.0 + dx, t.1 + dy);
                if (dx, dy) == (0, 0) || !open(n) || steps[at(n)] != u32::MAX {
                    continue;
                }
                if dx != 0 && dy != 0 {
                    let (side_x, side_y) = (open((n.0, t.1)), open((t.0, n.1)));
                    let passes = if jps {
                        side_x || side_y
                    } else {
                        side_x && side_y
                    };
                    if !passes {
                        continue;
                    }
                }
                steps[at(n)] = steps[at(t)] + 1;
                queue.push_back(n);
            }
        }
    }
    steps
}

/// E3b: `productionReachable` of `Blocks::canAddBlock`, which BWEB never
/// calls, for block `i` of `plan` as BWEB accepted it: from below each large
/// piece, a dragoon's JPS path to the center of each choke point of the area
/// of the block, outside the block (its right and bottom edges inclusive, as
/// in the code), `blockGrid` and the reserves, nearer the target than the
/// source is by 5 tiles.
fn production_reachable(
    plan: &Plan,
    cx: &Context<'_>,
    i: usize,
    size: (i32, i32),
    fail: &mut impl FnMut(&'static str, String, String),
) {
    const PROPERTY: &str = "bweb_production_reachable";
    const DRAGOON: usize = 3;
    let block = &plan.blocks[i];
    let subject = format!("block {},{}", block.tile.0, block.tile.1);

    // The reserves when the block was accepted, and after all blocks, which
    // must be those of stage 3.
    let mut reserved = bweb::before_blocks(plan, size);
    let add = |reserved: &mut Vec<bool>, b: &Block| {
        for (at, wh) in bweb::block_reserves(plan.race, b) {
            bweb::mark(reserved, size, at, wh);
        }
    };
    for b in &plan.blocks[..i] {
        add(&mut reserved, b);
    }
    let mut all = reserved.clone();
    for b in &plan.blocks[i..] {
        add(&mut all, b);
    }
    if all != plan.snapshots[3].reserved {
        fail(
            PROPERTY,
            subject,
            "the reserves of the blocks do not give those after the blocks".to_owned(),
        );
        return;
    }

    // `blockGrid`: `Blocks::initialize`, then the blocks before.
    let mut grid = vec![false; (size.0 * size.1) as usize];
    for s in &plan.stations {
        bweb::mark(&mut grid, size, s.location, DEPOT);
        for &d in &s.defenses {
            bweb::mark(&mut grid, size, d, (2, 2));
        }
        let base = cx.analysis.bases.iter().find(|b| b.location == s.location);
        let center = (s.location.0 * 32 + 64, s.location.1 * 32 + 48);
        for id in base.map_or(&[][..], |b| &b.minerals) {
            let Some(m) = cx.terrain.neutrals.iter().find(|n| n.unit_id == *id) else {
                continue;
            };
            let pos = (m.tile.0 * 32 + m.size.0 * 16, m.tile.1 * 32 + m.size.1 * 16);
            let halfway = ((pos.0 + center.0) / 2, (pos.1 + center.1) / 2);
            let tile = (halfway.0 / 32, halfway.1 / 32);
            bweb::mark(&mut grid, size, (tile.0 - 1, tile.1 - 1), (2, 2));
        }
    }
    for b in &plan.blocks[..i] {
        bweb::mark(&mut grid, size, b.tile, b.size);
    }

    let at = |t: Tile| (t.1 * size.0 + t.0) as usize;
    let on_map = |t: Tile| (0..size.0).contains(&t.0) && (0..size.1).contains(&t.1);
    let here = block.tile;
    let area = if on_map(here) {
        cx.analysis.tile_areas[at(here)]
    } else {
        0
    };
    let mut failed = Vec::new();
    if area <= 0 {
        failed.push("its tile in no area".to_owned());
    }
    for &piece in &block.large {
        let source = (piece.0, piece.1 + 3);
        for c in cx
            .analysis
            .choke_points
            .iter()
            .filter(|c| area > 0 && (c.areas.0 == area || c.areas.1 == area))
        {
            let target = (c.nodes[1].0 / 4, c.nodes[1].1 / 4);
            let distance =
                |t: Tile| f64::from((t.0 - target.0).pow(2) + (t.1 - target.1).pow(2)).sqrt();
            let limit = distance(source) + 5.0;
            let open = |t: Tile| {
                if !on_map(t) {
                    return false;
                }
                if t == source || t == target {
                    return true;
                }
                let outside = t.0 < here.0
                    || t.0 > here.0 + block.size.0
                    || t.1 < here.1
                    || t.1 > here.1 + block.size.1;
                plan.navigation.walkable[DRAGOON][at(t)]
                    && outside
                    && !grid[at(t)]
                    && !reserved[at(t)]
                    && distance(t) < limit
            };
            let steps = search(size, &open, &[source], true);
            if !on_map(target) || steps[at(target)] == u32::MAX {
                failed.push(format!(
                    "{},{} to choke point {} at {},{}",
                    source.0, source.1, c.index, target.0, target.1
                ));
            }
        }
    }
    if !failed.is_empty() {
        fail(
            PROPERTY,
            format!("{subject}: {}", failed.join("; ")),
            "a large piece reaches not every choke point of its area".to_owned(),
        );
    }
}
