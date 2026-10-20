//! Properties of the layout of BWEB: the geometry of its slots, its stations,
//! where it places buildings, its reserves and the state of the layout at
//! each stage of the analysis, `onStart → Stations → Walls → Blocks`.
//!
//! The layout is read at four stages: after `onStart` (0), the stations (1),
//! the wall (2) and the blocks (3). The stage before the blocks look for room,
//! after `findBlocks` reserves 6×5 around each main, cannot be read outside
//! BWEB: it is stage 2 with those reserves ([`before_blocks`]), and
//! `reserves_explained` checks that the blocks add nothing else.
//!
//! Walkability, the way out of a slot, paths and distances are not here.

use std::collections::BTreeSet;

use bwapi_sdk::bwem::NeutralKind;

use super::bwem::{Analysis, Terrain};
use super::{Offenders, Violation};

/// Race ids.
pub const ZERG: i32 = 0;
pub const TERRAN: i32 = 1;
pub const PROTOSS: i32 = 2;

/// `BlockType` ids of BWEB.
const PRODUCTION: i32 = 2;
const SUPPLY: i32 = 5;

type Tile = (i32, i32);

/// The tiles of the layout at a stage, row by row.
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    /// The type id of the building on the tile.
    pub used: Vec<Option<i32>>,
    pub reserved: Vec<bool>,
}

#[derive(Clone, Debug)]
pub struct Station {
    /// Of its base.
    pub location: Tile,
    pub main: bool,
    pub natural: bool,
    /// The index of its choke point.
    pub choke: Option<i32>,
    /// The ComSat of Terran or the Spire of a Zerg main.
    pub small: Option<Tile>,
    /// The Spawning Pool of a Zerg main.
    pub medium: Option<Tile>,
    pub defenses: Vec<Tile>,
}

#[derive(Clone, Debug, Default)]
pub struct Wall {
    pub choke: Option<i32>,
    pub area: Option<i32>,
    pub small: Vec<Tile>,
    pub medium: Vec<Tile>,
    pub large: Vec<Tile>,
    pub openings: Vec<Tile>,
    /// By layer, 1 to 3.
    pub defenses: Vec<(i32, Vec<Tile>)>,
}

#[derive(Clone, Debug)]
pub struct Block {
    pub tile: Tile,
    pub size: (i32, i32),
    /// `BlockType`.
    pub kind: i32,
    /// Addons among them.
    pub small: Vec<Tile>,
    pub medium: Vec<Tile>,
    pub large: Vec<Tile>,
    pub wide: Vec<Tile>,
}

/// What BWEB made of a map as a race.
#[derive(Clone, Debug)]
pub struct Plan {
    pub race: i32,
    /// The start location of the bot.
    pub start: Tile,
    pub stations: Vec<Station>,
    pub starting_main: Option<Tile>,
    pub starting_natural: Option<Tile>,
    pub wall: Option<Wall>,
    /// In the order they were made.
    pub blocks: Vec<Block>,
    /// After `onStart`, the stations, the wall and the blocks.
    pub snapshots: Vec<Snapshot>,
    pub navigation: Navigation,
}

/// What BWEB answers on the layout after the blocks: walkability, the
/// searches of `Path`, ground distances and closest choke tiles.
#[derive(Clone, Debug, Default)]
pub struct Navigation {
    /// `isWalkable` by size class, every tile: full, small, medium, large.
    pub walkable: Vec<Vec<bool>>,
    /// In the order they ran, with one cache of unreachable tiles.
    pub searches: Vec<Search>,
    /// Positions in pixels the distances and closest tiles are between and
    /// from; empty where they were not asked.
    pub points: Vec<(i32, i32)>,
    /// `getGroundDistance`, point by point; `None` for no distance.
    pub ground_distances: Vec<Option<f64>>,
    /// The choke points of the path of BWEM, point by point.
    pub choke_paths: Vec<Vec<i32>>,
    /// The length of that path, point by point; `None` for no path.
    pub bwem_lengths: Vec<Option<i32>>,
    /// `getClosestChokeTile` by choke point index, point by point; `None` for
    /// none.
    pub closest_choke_tiles: Vec<(i32, Vec<Option<Tile>>)>,
}

/// A search of `Path`.
#[derive(Clone, Debug)]
pub struct Search {
    /// The size class of the unit: 0 full, 1 small, 2 medium, 3 large.
    pub class: usize,
    /// 0 A* over `terrainWalkable`, 1 A* with the cost of [`search_cost`]
    /// over it, 2 JPS over it, 3 JPS over `unitWalkable`.
    pub mode: i32,
    pub source: Tile,
    pub target: Tile,
    pub reachable: bool,
    pub distance: f64,
    pub tiles: Vec<Tile>,
}

/// The cost of entering a tile in the searches of mode 1.
pub fn search_cost(t: Tile) -> f64 {
    f64::from((t.0 * 7 + t.1 * 13) % 5) * 8.0
}

/// The size class `isWalkable` picks for a unit `width` × `height` pixels.
pub fn size_class(width: i32, height: i32) -> usize {
    if width >= 30 || height >= 30 {
        3
    } else if width >= 20 || height >= 20 {
        2
    } else if width >= 10 || height >= 10 {
        1
    } else {
        0
    }
}

/// What the game tells at the start for the race: creep, and `canBuildHere`
/// of its resource depot, both by tile.
#[derive(Clone, Debug)]
pub struct Start {
    pub creep: Vec<bool>,
    pub depot: Vec<bool>,
}

/// What a check reads besides the plan.
pub struct Context<'a> {
    /// The name under `maps/`, without its extension.
    pub map: &'a str,
    pub analysis: &'a Analysis,
    pub terrain: &'a Terrain,
    pub start: &'a Start,
}

/// A building slot or an opening, as large as its type.
#[derive(Clone, Debug)]
struct Slot {
    owner: String,
    what: &'static str,
    at: Tile,
    size: (i32, i32),
    /// The building needs creep.
    creep: bool,
    /// A resource depot.
    depot: bool,
    /// The tiny minerals BWEM gives the base of a station's depot to mine out
    /// first (`Base::BlockingMinerals`): in the way of the depot by design.
    mine_out: Vec<i32>,
}

impl Slot {
    fn new(owner: &str, what: &'static str, at: Tile, size: (i32, i32)) -> Slot {
        Slot {
            owner: owner.to_owned(),
            what,
            at,
            size,
            creep: false,
            depot: false,
            mine_out: Vec::new(),
        }
    }

    fn name(&self) -> String {
        format!(
            "{} {} at {},{}",
            self.owner, self.what, self.at.0, self.at.1
        )
    }

    fn tiles(&self) -> impl Iterator<Item = Tile> + '_ {
        (0..self.size.1)
            .flat_map(move |dy| (0..self.size.0).map(move |dx| (self.at.0 + dx, self.at.1 + dy)))
    }

    fn overlaps(&self, other: &Slot) -> bool {
        overlap(self.at, self.size, other.at, other.size)
    }

    fn same_place(&self, other: &Slot) -> bool {
        self.at == other.at && self.size == other.size
    }
}

fn overlap(a: Tile, a_size: (i32, i32), b: Tile, b_size: (i32, i32)) -> bool {
    a.0 < b.0 + b_size.0 && b.0 < a.0 + a_size.0 && a.1 < b.1 + b_size.1 && b.1 < a.1 + a_size.1
}

fn station_name(s: &Station) -> String {
    format!("station {},{}", s.location.0, s.location.1)
}

/// The slots of a station: its depot, its secondary slots, its defenses.
fn station_slots(race: i32, s: &Station, analysis: &Analysis) -> Vec<Slot> {
    let owner = station_name(s);
    let zerg = race == ZERG;
    let mine_out = analysis
        .bases
        .iter()
        .find(|b| b.location == s.location)
        .map(|b| b.blocking_minerals.clone())
        .unwrap_or_default();
    let mut slots = vec![Slot {
        depot: true,
        mine_out,
        ..Slot::new(&owner, "depot", s.location, (4, 3))
    }];
    if let Some(at) = s.small {
        let what = if race == TERRAN { "comsat" } else { "spire" };
        slots.push(Slot {
            creep: zerg,
            ..Slot::new(&owner, what, at, (2, 2))
        });
    }
    if let Some(at) = s.medium {
        slots.push(Slot {
            creep: zerg,
            ..Slot::new(&owner, "pool", at, (3, 2))
        });
    }
    for &at in &s.defenses {
        slots.push(Slot {
            creep: zerg,
            ..Slot::new(&owner, "defense", at, (2, 2))
        });
    }
    slots
}

/// The buildings of the wall, its defenses of every layer, its openings.
fn wall_slots(race: i32, w: &Wall) -> (Vec<Slot>, Vec<Slot>, Vec<Slot>) {
    let owner = "wall";
    let zerg = race == ZERG;
    let mut pieces = Vec::new();
    for &at in &w.small {
        pieces.push(Slot::new(owner, "small piece", at, (2, 2)));
    }
    for &at in &w.medium {
        pieces.push(Slot {
            creep: zerg,
            ..Slot::new(owner, "medium piece", at, (3, 2))
        });
    }
    for &at in &w.large {
        pieces.push(Slot {
            depot: zerg,
            ..Slot::new(owner, "large piece", at, (4, 3))
        });
    }
    let all: BTreeSet<Tile> = w
        .defenses
        .iter()
        .flat_map(|(_, t)| t.iter().copied())
        .collect();
    let defenses = all
        .into_iter()
        .map(|at| Slot {
            creep: zerg,
            ..Slot::new(owner, "defense", at, (2, 2))
        })
        .collect();
    let openings = w
        .openings
        .iter()
        .map(|&at| Slot::new(owner, "opening", at, (1, 1)))
        .collect();
    (pieces, defenses, openings)
}

fn block_name(b: &Block) -> String {
    format!("block {},{}", b.tile.0, b.tile.1)
}

/// The slots of a block; a small one right of a large one, a row down, is
/// its addon.
fn block_slots(race: i32, b: &Block) -> Vec<Slot> {
    let owner = block_name(b);
    let zerg = race == ZERG;
    let mut slots = Vec::new();
    for &at in &b.small {
        let addon = b.large.contains(&(at.0 - 4, at.1 - 1));
        let what = if addon { "addon" } else { "small" };
        slots.push(Slot {
            creep: zerg,
            ..Slot::new(&owner, what, at, (2, 2))
        });
    }
    for &at in &b.medium {
        slots.push(Slot {
            creep: zerg,
            ..Slot::new(&owner, "medium", at, (3, 2))
        });
    }
    for &at in &b.large {
        slots.push(Slot {
            depot: zerg,
            ..Slot::new(&owner, "large", at, (4, 3))
        });
    }
    for &at in &b.wide {
        slots.push(Slot {
            creep: zerg,
            ..Slot::new(&owner, "wide", at, (4, 2))
        });
    }
    slots
}

/// What `createBlock` reserves for a slot: a large one but of Zerg and an
/// addon with a border of one.
fn block_reserve(race: i32, s: &Slot) -> (Tile, (i32, i32)) {
    match s.what {
        "large" if race != ZERG => ((s.at.0 - 1, s.at.1 - 1), (6, 5)),
        "addon" => ((s.at.0 - 1, s.at.1 - 1), (4, 4)),
        _ => (s.at, s.size),
    }
}

/// The slots of `b` with their sizes, addons 2×2.
pub fn block_footprints(race: i32, b: &Block) -> Vec<(Tile, (i32, i32))> {
    block_slots(race, b)
        .iter()
        .map(|s| (s.at, s.size))
        .collect()
}

/// What `createBlock` reserves for the slots of `b`.
pub fn block_reserves(race: i32, b: &Block) -> Vec<(Tile, (i32, i32))> {
    block_slots(race, b)
        .iter()
        .map(|s| block_reserve(race, s))
        .collect()
}

/// The reserves of `findBlocks` before its first block: 6×5 around each main.
fn main_reserves(plan: &Plan) -> Vec<(Tile, (i32, i32))> {
    plan.stations
        .iter()
        .filter(|s| s.main)
        .map(|s| ((s.location.0 - 1, s.location.1 - 1), (6, 5)))
        .collect()
}

/// The reserved tiles of stage 2 with those of `findBlocks` before its first
/// block.
pub fn before_blocks(plan: &Plan, size: (i32, i32)) -> Vec<bool> {
    let mut reserved = plan.snapshots[2].reserved.clone();
    for (at, (w, h)) in main_reserves(plan) {
        mark(&mut reserved, size, at, (w, h));
    }
    reserved
}

/// Marks the tiles of the rectangle at `at` on the map in `grid`, row by row.
pub fn mark(grid: &mut [bool], size: (i32, i32), at: Tile, (w, h): (i32, i32)) {
    for y in at.1..at.1 + h {
        for x in at.0..at.0 + w {
            if (0..size.0).contains(&x) && (0..size.1).contains(&y) {
                grid[(y * size.0 + x) as usize] = true;
            }
        }
    }
}

/// The violations of `plan`.
pub fn check(plan: &Plan, cx: &Context<'_>) -> Vec<Violation> {
    let mut out = Vec::new();
    let mut fail = |property: &'static str, subject: String, detail: String| {
        out.push(Violation {
            property: property.to_owned(),
            subject,
            detail,
        });
    };
    if plan.snapshots.len() != 4 {
        fail(
            "layout_stages",
            "layout".into(),
            format!("{} snapshots", plan.snapshots.len()),
        );
        return out;
    }
    let size = cx.analysis.size;
    let in_map = |t: Tile| (0..size.0).contains(&t.0) && (0..size.1).contains(&t.1);
    let at = |t: Tile| (t.1 * size.0 + t.0) as usize;
    let race = plan.race;

    let stations: Vec<(&Station, Vec<Slot>)> = plan
        .stations
        .iter()
        .map(|s| (s, station_slots(race, s, cx.analysis)))
        .collect();
    let (wall_pieces, wall_defenses, wall_openings) = plan
        .wall
        .as_ref()
        .map(|w| wall_slots(race, w))
        .unwrap_or_default();
    let blocks: Vec<(&Block, Vec<Slot>)> = plan
        .blocks
        .iter()
        .map(|b| (b, block_slots(race, b)))
        .collect();
    let building_slots = || {
        stations
            .iter()
            .flat_map(|(_, s)| s.iter())
            .chain(&wall_pieces)
            .chain(&wall_defenses)
            .chain(blocks.iter().flat_map(|(_, s)| s.iter()))
    };

    // G1: every slot on the map.
    for slot in building_slots().chain(&wall_openings) {
        if let Some(t) = slot.tiles().find(|&t| !in_map(t)) {
            fail(
                "slot_on_map",
                slot.name(),
                format!("tile {},{} off the map", t.0, t.1),
            );
        }
    }

    // G2: the slots of a block within it and apart.
    for (b, slots) in &blocks {
        for s in slots {
            if s.tiles().any(|t| !overlap(t, (1, 1), b.tile, b.size)) {
                fail(
                    "block_slot_inside",
                    s.name(),
                    format!("block of {}x{}", b.size.0, b.size.1),
                );
            }
        }
        apart(slots, "block_slots_apart", &mut fail);
    }

    // G3 and R1a: the parts of the wall apart; the slots of a station apart.
    let wall_parts: Vec<Slot> = wall_pieces
        .iter()
        .chain(&wall_defenses)
        .chain(&wall_openings)
        .cloned()
        .collect();
    apart(&wall_parts, "wall_parts_apart", &mut fail);
    for (_, slots) in &stations {
        apart(slots, "station_slots_apart", &mut fail);
    }

    // G4: references to BWEM.
    let base_area = |t: Tile| {
        cx.analysis
            .bases
            .iter()
            .find(|b| b.location == t)
            .map(|b| b.area)
    };
    let choke = |i: i32| cx.analysis.choke_points.iter().find(|c| c.index == i);
    for (s, _) in &stations {
        if base_area(s.location).is_none() {
            fail(
                "station_on_base",
                station_name(s),
                "no base of BWEM there".into(),
            );
        }
        if let Some(i) = s.choke
            && choke(i).is_none()
        {
            fail(
                "station_choke_exists",
                station_name(s),
                format!("choke point {i}"),
            );
        }
    }
    if let Some(w) = &plan.wall {
        let area_ok = w
            .area
            .is_some_and(|a| cx.analysis.areas.iter().any(|x| x.id == a));
        let choke_ok = w.choke.is_some_and(|i| choke(i).is_some());
        if !(choke_ok && area_ok) {
            fail(
                "wall_refs",
                "wall".into(),
                format!("choke point {:?}, area {:?}", w.choke, w.area),
            );
        }
    }
    for (name, loc) in [
        ("main", plan.starting_main),
        ("natural", plan.starting_natural),
    ] {
        if let Some(t) = loc
            && !plan.stations.iter().any(|s| s.location == t)
        {
            fail(
                "starting_station_exists",
                format!("starting {name}"),
                format!("{t:?}"),
            );
        }
    }

    // T1: a station per base, and no more; a natural is no main; mains on
    // start locations.
    for b in &cx.analysis.bases {
        let n = plan
            .stations
            .iter()
            .filter(|s| s.location == b.location)
            .count();
        let subject = format!("base {},{}", b.location.0, b.location.1);
        if n == 0 {
            fail("base_has_station", subject, "no station".into());
        } else if n > 1 {
            fail(
                "base_station_at_most_once",
                subject,
                format!("{n} stations"),
            );
        }
    }
    for (s, _) in &stations {
        if s.main && s.natural {
            fail("station_main_or_natural", station_name(s), "both".into());
        }
        let starting = cx
            .analysis
            .bases
            .iter()
            .any(|b| b.location == s.location && b.starting);
        if s.main != starting {
            fail(
                "bweb_station_main_starting",
                station_name(s),
                format!("main {}, starting base {starting}", s.main),
            );
        }
    }

    // T2: the starting main at the start of the bot; the starting natural, if
    // any, another natural. BWEB takes the start of the bot to be on a base of
    // BWEM: without one, what it takes is a diagnostic.
    let start_on_base = cx.analysis.bases.iter().any(|b| b.location == plan.start);
    if !start_on_base {
        fail(
            "diag_starting_main_without_base",
            "starting main".into(),
            format!(
                "{:?} for a start {:?} on no base",
                plan.starting_main, plan.start
            ),
        );
    } else if plan.starting_main != Some(plan.start) {
        fail(
            "starting_main",
            "starting main".into(),
            format!("{:?}, start {:?}", plan.starting_main, plan.start),
        );
    }
    if let Some(t) = plan.starting_natural {
        let natural = plan.stations.iter().any(|s| s.natural && s.location == t);
        if !natural || Some(t) == plan.starting_main {
            fail(
                "starting_natural",
                "starting natural".into(),
                format!("{t:?}"),
            );
        }
    }

    // T3: the choke point of a station is its way out of the area of its
    // base, so it borders that area; a station may have none.
    let mut off_area = Offenders::new("station_choke_borders_area", "stations");
    for (s, _) in &stations {
        if let (Some(i), Some(area)) = (s.choke, base_area(s.location))
            && let Some(c) = choke(i)
            && c.areas.0 != area
            && c.areas.1 != area
        {
            off_area.add(s.location, i, area, || {
                format!(
                    "{}{}, choke point {i} between areas {:?}, the base in {area}",
                    station_name(s),
                    if s.main { " (main)" } else { "" },
                    c.areas
                )
            });
        }
    }
    off_area.report(&mut fail);

    // P1a: static buildability.
    let mut obstacle = vec![false; (size.0 * size.1) as usize];
    for n in &cx.terrain.neutrals {
        let blocks_building = n.building
            || n.immobile
            || matches!(n.kind, NeutralKind::Mineral | NeutralKind::Geyser);
        if blocks_building {
            mark(&mut obstacle, size, n.tile, n.size);
        }
    }
    // The depot of a station may stand on the tiny minerals of its base to mine
    // out first: a plan, not a site to build on now.
    let mut mine_first = Vec::new();
    for slot in building_slots() {
        let mine_out = mine_out_tiles(slot, cx);
        let bad = slot.tiles().find(|&t| {
            !in_map(t)
                || !cx.terrain.buildable[at(t)]
                || (obstacle[at(t)] && !mine_out.contains(&t))
        });
        if slot.tiles().any(|t| mine_out.contains(&t)) {
            mine_first.push(slot.name());
        }
        if let Some(t) = bad {
            let why = if !in_map(t) {
                "off the map"
            } else if obstacle[at(t)] {
                "under a neutral"
            } else {
                "not buildable"
            };
            fail(
                "slot_buildable",
                slot.name(),
                format!("tile {},{} {why}", t.0, t.1),
            );
        }
    }

    for name in mine_first {
        fail(
            "diag_depot_on_minerals_to_mine",
            name,
            "a plan once they are mined out".into(),
        );
    }

    // P1b, a diagnostic until it is settled whether a slot of BWEB promises a
    // site `isPlaceable` accepts: `isPlaceable` on stage 0 of what P2 to P4
    // leave out, the slots of blocks and the depots of stations, but for
    // addons, which it is not the rule of.
    let l0_used: Vec<bool> = plan.snapshots[0].used.iter().map(Option::is_some).collect();
    let placeable = |slot: &Slot, used: &[bool]| placeable(slot, used, cx);
    let unchecked = stations
        .iter()
        .map(|(_, s)| &s[0])
        .chain(blocks.iter().flat_map(|(_, s)| s.iter()))
        .filter(|s| s.what != "addon");
    for slot in unchecked {
        if let Some(why) = refusal(slot, &l0_used, cx) {
            fail(
                "diag_slot_placeable",
                slot.name(),
                format!("after onStart: {why}"),
            );
        }
    }
    let mut off_creep = Offenders::new("diag_creep_slots_off_creep", "slots");
    for slot in building_slots().filter(|s| s.creep) {
        let off = slot
            .tiles()
            .filter(|&t| in_map(t) && !cx.start.creep[at(t)])
            .count();
        if off > 0 {
            off_creep.add(slot.at, off as i32, 0, || {
                format!("{}: {off} tiles off creep", slot.name())
            });
        }
    }
    off_creep.report(&mut fail);

    // P2: each defense and secondary slot of a station placeable when it was
    // added: stage 0, the depot and the slots added before it.
    for (s, slots) in &stations {
        let depot = &slots[0];
        let mut used = l0_used.clone();
        mark(&mut used, size, depot.at, depot.size);
        let secondary: Vec<&Slot> = slots
            .iter()
            .filter(|x| x.what == "spire" || x.what == "pool")
            .collect();
        for slot in &secondary {
            if !placeable(slot, &used) {
                fail(
                    "bweb_station_slot_placeable",
                    slot.name(),
                    "with the depot".into(),
                );
            }
        }
        for slot in &secondary {
            mark(&mut used, size, slot.at, slot.size);
        }
        let defenses: Vec<&Slot> = slots.iter().filter(|x| x.what == "defense").collect();
        for (i, slot) in defenses.iter().enumerate() {
            let mut state = used.clone();
            for (j, other) in defenses.iter().enumerate() {
                if i != j {
                    mark(&mut state, size, other.at, other.size);
                }
            }
            if !placeable(slot, &state) {
                fail(
                    "bweb_station_slot_placeable",
                    slot.name(),
                    format!("with the depot and the other slots of {}", station_name(s)),
                );
            }
        }
    }

    // P3: the wall on stage 1.
    if plan.wall.is_some() {
        let l1 = &plan.snapshots[1];
        let l1_used: Vec<bool> = l1.used.iter().map(Option::is_some).collect();
        let reserved = |slot: &Slot| slot.tiles().any(|t| in_map(t) && l1.reserved[at(t)]);
        let station_defense = |slot: &Slot| {
            stations
                .iter()
                .any(|(_, s)| s.iter().any(|x| x.what == "defense" && x.same_place(slot)))
        };
        for slot in &wall_pieces {
            if reserved(slot) {
                fail(
                    "bweb_wall_piece_free",
                    slot.name(),
                    "on reserved tiles of stage 1".into(),
                );
            }
            let can_build =
                !slot.depot || cx.start.depot.get(at(slot.at)).copied().unwrap_or(false);
            if !placeable(slot, &l1_used) || !can_build {
                fail(
                    "bweb_wall_piece_placeable",
                    slot.name(),
                    "on stage 1".into(),
                );
            }
        }
        let w = plan.wall.as_ref().expect("a wall");
        for (layer, tiles) in &w.defenses {
            for &t in tiles {
                let slot = Slot {
                    creep: race == ZERG,
                    ..Slot::new("wall", "defense", t, (2, 2))
                };
                if !placeable(&slot, &l1_used) {
                    fail(
                        "bweb_wall_defense_placeable",
                        slot.name(),
                        format!("layer {layer}"),
                    );
                }
                if *layer >= 2 && reserved(&slot) && !station_defense(&slot) {
                    fail(
                        "bweb_wall_defense_free",
                        slot.name(),
                        format!("layer {layer}, reserved on stage 1"),
                    );
                }
            }
        }
        for slot in &wall_openings {
            if !placeable(slot, &l1_used) {
                fail(
                    "bweb_wall_opening_placeable",
                    slot.name(),
                    "for a Dragoon on stage 1".into(),
                );
            }
        }
    }

    // P4 and R2: each block where `canAddBlock` put it, against stage 2 with
    // the reserves of `findBlocks` and of the blocks before it.
    let mut reserved = before_blocks(plan, size);
    let starting_choke = plan
        .stations
        .iter()
        .find(|s| Some(s.location) == plan.starting_main)
        .and_then(|s| s.choke)
        .and_then(choke);
    for (b, slots) in &blocks {
        let name = block_name(b);
        let border = i32::from((b.kind != SUPPLY && race != ZERG) || b.kind == PRODUCTION);
        let room = (b.tile.0 - border, b.tile.1 - border);
        let room_size = (b.size.0 + 2 * border, b.size.1 + 2 * border);
        let bad = (0..room_size.1)
            .flat_map(|dy| (0..room_size.0).map(move |dx| (room.0 + dx, room.1 + dy)))
            .find(|&t| !in_map(t) || !cx.terrain.buildable[at(t)] || reserved[at(t)]);
        if let Some(t) = bad {
            fail(
                "bweb_block_room",
                name.clone(),
                format!("tile {},{} with border {border}", t.0, t.1),
            );
        }
        for s in slots {
            let (r, r_size) = block_reserve(race, s);
            let on_reserved = (0..r_size.1)
                .flat_map(|dy| (0..r_size.0).map(move |dx| (r.0 + dx, r.1 + dy)))
                .any(|t| in_map(t) && reserved[at(t)]);
            if on_reserved {
                fail(
                    "bweb_block_slot_free",
                    s.name(),
                    "reserved before the block".into(),
                );
            }
        }
        if b.kind == SUPPLY {
            let high = (0..b.size.1)
                .flat_map(|dy| (0..b.size.0).map(move |dx| (b.tile.0 + dx, b.tile.1 + dy)))
                .find(|&t| in_map(t) && min_altitude(cx.analysis, t) > 170);
            if let Some(t) = high {
                fail(
                    "bweb_supply_low",
                    name.clone(),
                    format!("tile {},{}", t.0, t.1),
                );
            }
            // `canAddBlock` takes the top left plus the size for the center.
            if let Some(c) = starting_choke {
                let center = (b.tile.0 * 32 + b.size.0 * 32, b.tile.1 * 32 + b.size.1 * 32);
                let middle = (c.nodes[1].0 * 8, c.nodes[1].1 * 8);
                let d =
                    f64::from((center.0 - middle.0).pow(2) + (center.1 - middle.1).pow(2)).sqrt();
                if d < 160.0 {
                    fail(
                        "bweb_supply_away_from_choke",
                        name.clone(),
                        format!("{d:.1} pixels"),
                    );
                }
            }
        }
        if race == ZERG {
            let (w, h) = b.size;
            let corners = [(0, 0), (w - 1, 0), (0, h - 1), (w - 1, h - 1)];
            let creep = corners.iter().all(|&(dx, dy)| {
                let t = (b.tile.0 + dx, b.tile.1 + dy);
                in_map(t) && cx.start.creep[at(t)]
            });
            if b.kind != PRODUCTION && !creep {
                fail(
                    "bweb_zerg_block_creep",
                    name.clone(),
                    "a corner off creep".into(),
                );
            }
            for s in slots.iter().filter(|s| s.what == "large") {
                if !cx.start.depot.get(at(s.at)).copied().unwrap_or(false) {
                    fail(
                        "bweb_zerg_hatchery_here",
                        s.name(),
                        "canBuildHere fails".into(),
                    );
                }
            }
        }
        for s in slots {
            let (r, r_size) = block_reserve(race, s);
            mark(&mut reserved, size, r, r_size);
        }
    }

    // R1b: what is used at once does not overlap: blocks with each other, the
    // wall and the stations; the buildings and openings of the wall with the
    // stations but where a piece is a defense of a station.
    let station_slots: Vec<&Slot> = stations.iter().flat_map(|(_, s)| s.iter()).collect();
    for (i, (_, slots)) in blocks.iter().enumerate() {
        let others = blocks[i + 1..]
            .iter()
            .flat_map(|(_, s)| s.iter())
            .chain(&wall_parts)
            .chain(station_slots.iter().copied());
        for o in others {
            if let Some(s) = slots.iter().find(|s| s.overlaps(o)) {
                fail(
                    "objects_apart",
                    format!("{} and {}", s.name(), o.name()),
                    String::new(),
                );
            }
        }
    }
    for p in wall_pieces.iter().chain(&wall_openings) {
        for o in &station_slots {
            if p.overlaps(o) && !(o.what == "defense" && p.same_place(o)) {
                fail(
                    "objects_apart",
                    format!("{} and {}", p.name(), o.name()),
                    String::new(),
                );
            }
        }
    }

    // R1c, diagnostics: stations with each other; defenses of the wall with
    // stations but where they are the same defense.
    // Two stations of one base are `base_station_at_most_once`, not overlaps.
    for (i, (sa, a)) in stations.iter().enumerate() {
        for (sb, b) in &stations[i + 1..] {
            if sa.location == sb.location {
                continue;
            }
            for x in a {
                if let Some(y) = b.iter().find(|y| x.overlaps(y)) {
                    fail(
                        "diag_stations_overlap",
                        format!("{} and {}", x.name(), y.name()),
                        String::new(),
                    );
                }
            }
        }
    }
    // A defense of the wall on the depot of the station of the wall: two
    // buildings of one plan that cannot both stand. Strict; other overlaps of
    // wall defenses with stations, a diagnostic.
    let wall_area = plan.wall.as_ref().and_then(|w| w.area);
    for d in &wall_defenses {
        for (s, slots) in &stations {
            for o in slots {
                if !d.overlaps(o) || (o.what == "defense" && d.same_place(o)) {
                    continue;
                }
                let own_depot = o.what == "depot" && base_area(s.location) == wall_area;
                let property = if own_depot {
                    "wall_defense_off_station_depot"
                } else {
                    "diag_wall_defense_on_station"
                };
                fail(
                    property,
                    format!("{} and {}", d.name(), o.name()),
                    String::new(),
                );
            }
        }
    }

    check_layout(plan, cx, &stations, &wall_parts, &blocks, &mut fail);
    out
}

/// No two slots of `slots` overlap.
fn apart(
    slots: &[Slot],
    property: &'static str,
    fail: &mut impl FnMut(&'static str, String, String),
) {
    for (i, a) in slots.iter().enumerate() {
        for b in &slots[i + 1..] {
            if a.overlaps(b) {
                fail(
                    property,
                    format!("{} and {}", a.name(), b.name()),
                    String::new(),
                );
            }
        }
    }
}

/// The least altitude of the mini tiles of a tile.
fn min_altitude(a: &Analysis, t: Tile) -> i32 {
    let walk_width = a.size.0 * 4;
    (0..4)
        .flat_map(|dy| (0..4).map(move |dx| (t.0 * 4 + dx, t.1 * 4 + dy)))
        .map(|(x, y)| a.altitudes[(y * walk_width + x) as usize])
        .min()
        .unwrap_or(0)
}

/// `Map::isPlaceable` for the slot on a layout whose used tiles are `used`:
/// every tile on the map, buildable and not used, its top left mini tile
/// walkable; for a building on creep, the row below buildable; for a resource
/// depot, no mineral within 4 tiles left, 3 up, 6 right or 5 down of its top
/// left, no geyser within 6 left, 4 up, 6 right or 5 down.
fn placeable(slot: &Slot, used: &[bool], cx: &Context<'_>) -> bool {
    refusal(slot, used, cx).is_none()
}

/// Why `Map::isPlaceable` refuses the slot, as [`placeable`] checks it.
fn refusal(slot: &Slot, used: &[bool], cx: &Context<'_>) -> Option<String> {
    let size = cx.analysis.size;
    let in_map = |t: Tile| (0..size.0).contains(&t.0) && (0..size.1).contains(&t.1);
    let at = |t: Tile| (t.1 * size.0 + t.0) as usize;
    let buildable = |t: Tile| in_map(t) && cx.terrain.buildable[at(t)];
    let walk_width = size.0 * 4;
    if slot.creep {
        let below = slot.at.1 + slot.size.1;
        if let Some(x) = (slot.at.0..slot.at.0 + slot.size.0).find(|&x| !buildable((x, below))) {
            return Some(format!(
                "on creep, the tile {x},{below} below is not buildable"
            ));
        }
    }
    if slot.depot {
        let (x, y) = slot.at;
        let near = |t: Tile, left: i32, up: i32| {
            t.0 > x - left && t.1 > y - up && t.0 < x + 7 && t.1 < y + 6
        };
        let resource = cx
            .terrain
            .neutrals
            .iter()
            .filter(|n| !slot.mine_out.contains(&n.unit_id))
            .find(|n| match n.kind {
                NeutralKind::Mineral => near(n.top_left, 5, 4),
                NeutralKind::Geyser => near(n.top_left, 7, 5),
                _ => false,
            });
        if let Some(n) = resource {
            return Some(format!(
                "a resource depot near unit {} of {}",
                n.unit_id, n.amount
            ));
        }
    }
    for t in slot.tiles() {
        if !buildable(t) {
            return Some(format!("tile {},{} not buildable", t.0, t.1));
        }
        if !cx.terrain.walkable[(t.1 * 4 * walk_width + t.0 * 4) as usize] {
            return Some(format!(
                "the top left mini tile of {},{} unwalkable",
                t.0, t.1
            ));
        }
        if used[at(t)] && !mine_out_tiles(slot, cx).contains(&t) {
            return Some(format!("tile {},{} used", t.0, t.1));
        }
    }
    None
}

/// The tiles of the minerals the slot's base mines out first.
fn mine_out_tiles(slot: &Slot, cx: &Context<'_>) -> BTreeSet<Tile> {
    cx.terrain
        .neutrals
        .iter()
        .filter(|n| slot.mine_out.contains(&n.unit_id))
        .flat_map(|n| {
            (0..n.size.1)
                .flat_map(move |dy| (0..n.size.0).map(move |dx| (n.tile.0 + dx, n.tile.1 + dy)))
        })
        .collect()
}

/// The state of the layout from stage to stage.
fn check_layout(
    plan: &Plan,
    cx: &Context<'_>,
    stations: &[(&Station, Vec<Slot>)],
    wall_parts: &[Slot],
    blocks: &[(&Block, Vec<Slot>)],
    fail: &mut impl FnMut(&'static str, String, String),
) {
    let size = cx.analysis.size;
    let n = (size.0 * size.1) as usize;
    let tile = |i: usize| ((i as i32) % size.0, (i as i32) / size.0);
    let [l0, l1, l2, l3] = [0, 1, 2, 3].map(|k| &plan.snapshots[k]);

    // D1: `onStart` uses the tiles of neutral buildings, geysers and minerals,
    // and reserves those of neutrals that do not move.
    let mut used = vec![false; n];
    let mut reserved = vec![false; n];
    for nt in &cx.terrain.neutrals {
        match nt.kind {
            NeutralKind::Geyser => mark(&mut used, size, nt.tile, (4, 2)),
            NeutralKind::Mineral => mark(&mut used, size, nt.tile, (2, 1)),
            _ => {}
        }
        if nt.building {
            mark(&mut used, size, nt.tile, nt.size);
        }
        if nt.immobile {
            mark(&mut reserved, size, nt.tile, nt.size);
        }
    }
    let mut d1 = Offenders::new("bweb_layout_after_start", "tiles");
    for i in 0..n {
        let (u, r) = (l0.used[i].is_some(), l0.reserved[i]);
        if u != used[i] || r != reserved[i] {
            d1.add(
                tile(i),
                i32::from(u) + 2 * i32::from(r),
                i32::from(used[i]) + 2 * i32::from(reserved[i]),
                || {
                    format!(
                        "used {u} reserved {r}, the neutrals give used {} reserved {}",
                        used[i], reserved[i]
                    )
                },
            );
        }
    }
    d1.report(fail);

    // D2: the tiles of neutrals stay used.
    let mut d2 = Offenders::new("neutral_tiles_stay_used", "tiles");
    for (k, l) in [(1, l1), (2, l2), (3, l3)] {
        for i in (0..n).filter(|&i| used[i] && l.used[i].is_none()) {
            d2.add(tile(i), k, 1, || format!("free at stage {k}"));
        }
    }
    d2.report(fail);

    // D3: reserves only grow; used tiles do not change after the stations.
    let mut d3 = Offenders::new("layout_grows", "tiles");
    for (k, (a, b)) in [(1, (l0, l1)), (2, (l1, l2)), (3, (l2, l3))] {
        for i in 0..n {
            if a.reserved[i] && !b.reserved[i] {
                d3.add(tile(i), k, 0, || format!("unreserved at stage {k}"));
            }
            if k >= 2 && a.used[i] != b.used[i] {
                d3.add(tile(i), k, 1, || format!("used changed at stage {k}"));
            }
        }
    }
    d3.report(fail);

    // D4 and D5: what each stage reserves is what its objects declare.
    // Stage 1: the slots of the stations, the resources of their bases, and the
    // lines from depot to resource within the box of both.
    let mut declared1 = vec![false; n];
    let mut explained1 = vec![false; n];
    for (s, slots) in stations {
        for slot in slots {
            mark(&mut declared1, size, slot.at, slot.size);
        }
        let Some(base) = cx.analysis.bases.iter().find(|b| b.location == s.location) else {
            continue;
        };
        for id in base.minerals.iter().chain(&base.geysers) {
            let Some(r) = cx.terrain.neutrals.iter().find(|x| x.unit_id == *id) else {
                continue;
            };
            let reserve = if r.kind == NeutralKind::Geyser {
                (4, 2)
            } else {
                (2, 1)
            };
            mark(&mut declared1, size, r.top_left, reserve);
            let lo = (
                s.location.0.min(r.top_left.0),
                s.location.1.min(r.top_left.1),
            );
            let hi = (
                (s.location.0 + 4).max(r.top_left.0 + r.size.0),
                (s.location.1 + 3).max(r.top_left.1 + r.size.1),
            );
            mark(&mut explained1, size, lo, (hi.0 - lo.0, hi.1 - lo.1));
        }
    }
    let mut declared2 = vec![false; n];
    for slot in wall_parts {
        mark(&mut declared2, size, slot.at, slot.size);
    }
    let mut declared3 = vec![false; n];
    for (at, s) in main_reserves(plan) {
        mark(&mut declared3, size, at, s);
    }
    for (_, slots) in blocks {
        for slot in slots {
            let (r, s) = block_reserve(plan.race, slot);
            mark(&mut declared3, size, r, s);
        }
    }
    let mut d4 = Offenders::new("reserved_as_declared", "tiles");
    let mut d5 = Offenders::new("reserves_explained", "tiles");
    for (k, (before, after, declared, extra)) in [
        (1, (l0, l1, &declared1, Some(&explained1))),
        (2, (l1, l2, &declared2, None)),
        (3, (l2, l3, &declared3, None)),
    ] {
        for i in 0..n {
            if declared[i] && !after.reserved[i] {
                d4.add(tile(i), k, 1, || {
                    format!("declared, not reserved at stage {k}")
                });
            }
            let new = after.reserved[i] && !before.reserved[i];
            if new && !declared[i] && !extra.is_some_and(|e| e[i]) {
                d5.add(tile(i), k, 0, || {
                    format!("reserved at stage {k} by nothing declared")
                });
            }
        }
    }
    d4.report(fail);
    d5.report(fail);
}
