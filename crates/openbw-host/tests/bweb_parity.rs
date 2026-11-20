//! The Rust port of BWEB against BWEB v1.17 in C++, on the same game. Each map
//! runs as each race in a process of its own: C++ BWEM and BWEB keep globals,
//! and read the race of the bot.
//!
//! A resource depot is not compared where units or creep are near:
//! `canBuildHere` checks them, the port does not. Defense angles may differ in
//! the last bits, as `atan2` of the port and of glibc do.
//!
//! What C++ makes is also checked for the properties of the tests of
//! `bwapi-sdk` (`crates/sdk/tests/property`), against the analysis of BWEM of
//! the port, which `bwem_parity` finds alike.

#![cfg(bweb_reference)]

#[path = "../../sdk/tests/extract/mod.rs"]
mod extract;
#[path = "../../sdk/tests/property/mod.rs"]
mod property;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;
use std::time::Instant;

use bwapi::{Bot, Context, Position, Race, TilePosition, UnitType};
use bwapi_sdk::PositionExt;
use bwapi_sdk::adapter::{map_input, start_state};
use bwapi_sdk::bweb::{self, Blocks, Path, Pathfinding, Stations, WallSpec, Walls};
use bwapi_sdk::bwem::{ChokePoint, Map, MapInput};
use openbw_host::bweb_reference::{
    self, BlockRecord, Reference, SearchRecord, StationRecord, WallRecord,
};

const MAPS: &[&str] = &[
    "scai/(2)Showdown.scx",
    "scai/(4)Sparkle 1.1.scx",
    "scai/(2)Benzene.scx",
    "scai/(2)Destination.scx",
    "scai/(2)Heartbreak Ridge.scx",
    "scai/(3)Neo Moon Glaive.scx",
    "scai/(3)Tau Cross.scx",
    "scai/(4)Andromeda.scx",
    "scai/(4)Circuit Breaker.scx",
    "scai/(4)Electric Circuit.scx",
    "scai/(4)Empire of the Sun.scm",
    "scai/(4)Fighting Spirit.scx",
    "scai/(4)Icarus.scm",
    "scai/(4)Jade.scx",
    "scai/(4)La Mancha1.1.scx",
    "scai/(4)Python.scx",
    "scai/(4)Roadrunner.scx",
    // The Barracks of a game that its worker never reached.
    "scbw/(2)Road War.scm",
];

/// One of each size class of `isWalkable`: none, small, medium, large.
const WALK_TYPES: [UnitType; 4] = [
    UnitType::NONE,
    UnitType::ZERG_ZERGLING,
    UnitType::PROTOSS_PROBE,
    UnitType::PROTOSS_DRAGOON,
];

/// Sizes 2×2 to 4×3, on creep, and two resource depots.
const PLACE_TYPES: [UnitType; 6] = [
    UnitType::PROTOSS_PYLON,
    UnitType::TERRAN_SUPPLY_DEPOT,
    UnitType::PROTOSS_GATEWAY,
    UnitType::ZERG_SPAWNING_POOL,
    UnitType::TERRAN_COMMAND_CENTER,
    UnitType::ZERG_HATCHERY,
];

/// A small and a large unit for the searches of `Path`.
const PATH_TYPES: [UnitType; 2] = [UnitType::ZERG_ZERGLING, UnitType::PROTOSS_DRAGOON];

/// The bases searched between: every ordered pair of them.
const PATH_BASES: usize = 8;

/// The races the bot plays each map as.
const RACES: [Race; 3] = [Race::TERRAN, Race::ZERG, Race::PROTOSS];

/// The mode of the searches over `unitWalkable`, the only one that sees the
/// buildings the race places; the modes before it search the terrain.
const UNIT_MODE: i32 = 3;

const MAP_VAR: &str = "BWEB_PARITY_MAP";
const RACE_VAR: &str = "BWEB_PARITY_RACE";

static REPORT: Mutex<Option<Vec<String>>> = Mutex::new(None);

/// The properties of C++, as `property::report` prints them for the parent to
/// judge.
static PROPERTIES: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// What C++ found, by kind: a comparison of nothing passes.
static FOUND: Mutex<Vec<(&str, usize)>> = Mutex::new(Vec::new());

/// The seconds of each phase, as "C++ <phase>" or "port <phase>".
static SECONDS: Mutex<Vec<(String, f64)>> = Mutex::new(Vec::new());

struct Comparer;

impl<'game> Bot<'game> for Comparer {
    fn on_start(cx: Context<'_, 'game>) -> Self {
        let input = map_input(cx);
        let me = cx.game().self_player().expect("the bot plays");
        let race = std::env::var(RACE_VAR).ok().and_then(|r| r.parse().ok());
        // What does not depend on the race is compared as the first race only.
        let shared = race == Some(RACES[0].id());
        let start = Instant::now();
        let report = match Map::initialize(&input) {
            _ if race != Some(me.race().id()) => {
                vec![format!("the bot plays {:?}, not race {race:?}", me.race())]
            }
            Err(e) => vec![format!("the port refused the map: {e}")],
            Ok(mut bwem) => {
                let bases_found = bwem.find_bases_for_starting_locations();
                let bwem_seconds = start.elapsed().as_secs_f64();
                let start = Instant::now();
                let mut layout = bweb::Layout::new(&bwem, &input);
                let after_start = layout.clone();
                let stations =
                    Stations::analyze(&bwem, &mut layout, me.start_location(), me.race());
                let after_stations = layout.clone();
                let state = start_state(cx);
                let mut walls = Walls::new(&bwem);
                let enemy = cx.game().enemy().map_or(Race::NONE, |e| e.race());
                let _ = WallSpec::natural(me.race(), enemy, &bwem, &stations)
                    .and_then(|spec| walls.build(&bwem, &mut layout, &stations, &state, &spec));
                let after_wall = layout.clone();
                let blocks = Blocks::find(&bwem, &mut layout, &stations, me.race(), &state);
                let bweb_seconds = start.elapsed().as_secs_f64();
                let port = Port {
                    input: &input,
                    bwem: &bwem,
                    layout: &layout,
                    snapshots: [&after_start, &after_stations, &after_wall, &layout],
                    stations: &stations,
                    blocks: &blocks,
                    walls: &walls,
                    walk: WALK_TYPES.to_vec(),
                    place: PLACE_TYPES,
                };
                let points = if shared { port.points() } else { Vec::new() };
                let path_tiles = port.path_tiles();
                let first_mode = if shared { 0 } else { UNIT_MODE };
                let ids = |types: &[UnitType]| types.iter().map(|t| t.id()).collect::<Vec<_>>();
                let reference = bweb_reference::analyse(
                    &ids(&port.walk),
                    &ids(&port.place),
                    &points,
                    &PATH_TYPES.map(|t| t.id()),
                    &path_tiles,
                    first_mode,
                );
                match reference {
                    Some(reference) => {
                        let stations = &reference.stations;
                        *FOUND.lock().unwrap() = vec![
                            ("defenses", stations.iter().map(|s| s.defenses.len()).sum()),
                            (
                                "small slots",
                                stations.iter().filter(|s| s.small != (1000, 1000)).count(),
                            ),
                            (
                                "medium slots",
                                stations.iter().filter(|s| s.medium != (1000, 1000)).count(),
                            ),
                            ("blocks", reference.blocks.len()),
                            ("walls", usize::from(reference.wall.is_some())),
                        ];
                        let mut seconds = vec![
                            ("port BWEM".to_owned(), bwem_seconds),
                            ("port BWEB".to_owned(), bweb_seconds),
                        ];
                        let mut report = port.compare(&points, &reference, &mut seconds);
                        let lap = Instant::now();
                        report.extend(port.compare_searches(&path_tiles, first_mode, &reference));
                        seconds.push(("port searches".to_owned(), lap.elapsed().as_secs_f64()));
                        seconds.extend(
                            bweb_reference::PHASES
                                .iter()
                                .zip(&reference.seconds)
                                .map(|(phase, &s)| (format!("C++ {phase}"), s)),
                        );
                        *SECONDS.lock().unwrap() = seconds;
                        let analysis = extract::analysis(&bwem, bases_found);
                        let navigation =
                            navigation_of_cpp(&reference, &bwem, &path_tiles, first_mode, &points);
                        *PROPERTIES.lock().unwrap() = property::report(&[properties_of_cpp(
                            cx, &input, &analysis, &reference, navigation,
                        )]);
                        report
                    }
                    None => vec!["C++ threw".to_owned()],
                }
            }
        };
        *REPORT.lock().unwrap() = Some(report);
        Comparer
    }
}

bwapi::bot!(Comparer);

/// What C++ made, as the properties take it.
fn plan_of_cpp(r: &Reference, race: i32, start: TilePosition) -> property::bweb::Plan {
    use property::bweb::{Block, Plan, Snapshot, Station, Wall};
    // `TilePositions::Invalid`.
    let slot = |t: (i32, i32)| (t != (1000, 1000)).then_some(t);
    Plan {
        race,
        start: (start.x, start.y),
        stations: r
            .stations
            .iter()
            .map(|s| Station {
                location: s.location,
                main: s.main,
                natural: s.natural,
                choke: s.choke,
                small: slot(s.small),
                medium: slot(s.medium),
                defenses: s.defenses.clone(),
            })
            .collect(),
        starting_main: r.starting_main,
        starting_natural: r.starting_natural,
        wall: r.wall.as_ref().map(|w| Wall {
            choke: w.choke,
            area: w.area,
            small: w.pieces[0].clone(),
            medium: w.pieces[1].clone(),
            large: w.pieces[2].clone(),
            openings: w.pieces[3].clone(),
            defenses: w
                .defenses
                .iter()
                .filter(|(row, _)| *row > 0)
                .cloned()
                .collect(),
        }),
        blocks: r
            .blocks
            .iter()
            .map(|b| Block {
                tile: b.tile,
                size: b.size,
                kind: b.kind,
                small: b.slots[0].clone(),
                medium: b.slots[1].clone(),
                large: b.slots[2].clone(),
                wide: b.slots[3].clone(),
            })
            .collect(),
        snapshots: r
            .snapshots
            .iter()
            .map(|s| Snapshot {
                used: s.used.clone(),
                reserved: s.reserved.clone(),
            })
            .collect(),
        navigation: property::bweb::Navigation::default(),
    }
}

/// What C++ answers of the navigation, as the properties take it: the searches
/// in the order of `bweb_reference::analyse` between `path_tiles` from
/// `first_mode` on, the distances and closest tiles from `points`, with the
/// lengths of the paths of BWEM of the port, which C++ does not give.
fn navigation_of_cpp(
    r: &Reference,
    bwem: &Map,
    path_tiles: &[(i32, i32)],
    first_mode: i32,
    points: &[(i32, i32)],
) -> property::bweb::Navigation {
    use property::bweb::{Navigation, Search, size_class};
    let mut records = r.searches.iter();
    let mut searches = Vec::new();
    for ty in PATH_TYPES {
        let class = size_class(ty.width(), ty.height());
        for &source in path_tiles {
            for &target in path_tiles {
                for mode in first_mode..=UNIT_MODE {
                    let Some(s) = records.next() else { break };
                    searches.push(Search {
                        class,
                        mode,
                        source,
                        target,
                        reachable: s.reachable,
                        distance: s.distance,
                        tiles: s.tiles.clone(),
                    });
                }
            }
        }
    }
    let positions: Vec<Position> = points.iter().map(|&(x, y)| Position::new(x, y)).collect();
    let mut by_index = r.closest_choke_tiles.clone();
    by_index.sort_by_key(|(cp, _)| *cp);
    Navigation {
        walkable: r.walkable.clone(),
        searches,
        points: points.to_vec(),
        // `DBL_MAX` for none.
        ground_distances: r
            .ground_distances
            .iter()
            .map(|&d| (d != f64::MAX).then_some(d))
            .collect(),
        choke_paths: r.paths.clone(),
        bwem_lengths: positions
            .iter()
            .flat_map(|&a| {
                positions
                    .iter()
                    .map(move |&b| bwem.choke_point_path(a, b).and_then(|p| p.length()))
            })
            .collect(),
        closest_choke_tiles: by_index
            .iter()
            .map(|(cp, tiles)| {
                // `Positions::Invalid` for none.
                let found = tiles.iter().map(|&t| (t != (32000, 32000)).then_some(t));
                (*cp, found.collect())
            })
            .collect(),
    }
}

/// What C++ breaks of the properties of BWEB.
fn properties_of_cpp(
    cx: Context<'_, '_>,
    input: &MapInput,
    analysis: &property::bwem::Analysis,
    r: &Reference,
    navigation: property::bweb::Navigation,
) -> (property::Case, Vec<property::Violation>) {
    let game = cx.game();
    let me = game.self_player().expect("the bot plays");
    let (width, height) = (game.map_width(), game.map_height());
    let tiles = || (0..height).flat_map(move |y| (0..width).map(move |x| TilePosition::new(x, y)));
    let depot = me.race().resource_depot();
    let start = property::bweb::Start {
        creep: tiles().map(|t| game.has_creep(t)).collect(),
        depot: tiles().map(|t| game.can_build_here(t, depot)).collect(),
    };
    let terrain = property::bwem::Terrain::new(input, &[]);
    let mut plan = plan_of_cpp(r, me.race().id(), me.start_location());
    plan.navigation = navigation;
    let path = std::env::var_os(MAP_VAR).expect("BWEB_PARITY_MAP is the path of the map");
    let name = openbw_host::map_name(std::path::Path::new(&path));
    let map = name.rsplit_once('.').map_or(name.as_str(), |(n, _)| n);
    let context = property::bweb::Context {
        map,
        analysis,
        terrain: &terrain,
        start: &start,
    };
    let race = match me.race() {
        Race::ZERG => "zerg",
        Race::PROTOSS => "protoss",
        _ => "terran",
    };
    let case = property::Case {
        side: property::Side::Cpp,
        map: map.to_owned(),
        race: race.to_owned(),
        state: "analysis".to_owned(),
    };
    let mut violations = property::bweb::check(&plan, &context);
    violations.extend(property::nav::check(&plan, &context));
    violations.extend(property::production::check(&plan, &context));
    (case, violations)
}

unsafe extern "C" {
    /// Ends the process without running destructors: C++ BWEM's singleton
    /// crashes in its destructor on Sparkle 1.1.
    fn _exit(status: i32) -> !;
}

/// One map as one race, in a child process of `the_port_matches_bweb`.
#[test]
#[ignore = "run by the_port_matches_bweb, one process per map and race"]
fn one_map() {
    let map = std::env::var_os(MAP_VAR).expect("BWEB_PARITY_MAP is the path of the map");
    let race = std::env::var(RACE_VAR).expect("BWEB_PARITY_RACE names the race");
    let race: i32 = race.parse().expect("BWEB_PARITY_RACE is a race id");
    openbw_host::play_as(std::path::Path::new(&map), 1, race, gameInit, newAIModule).unwrap();
    let report = REPORT
        .lock()
        .unwrap()
        .take()
        .expect("the bot never started");
    for line in &report {
        println!("PARITY {line}");
    }
    for (kind, count) in FOUND.lock().unwrap().iter() {
        println!("PARITY_FOUND {kind}: {count}");
    }
    for (phase, seconds) in SECONDS.lock().unwrap().iter() {
        println!("PARITY_SECONDS {phase}: {seconds}");
    }
    for line in PROPERTIES.lock().unwrap().iter() {
        println!("{line}");
    }
    println!("PARITY_DONE");
    // SAFETY: the report is printed; nothing else is left to do.
    unsafe { _exit(i32::from(!report.is_empty())) }
}

#[test]
fn the_port_matches_bweb() {
    let exe = std::env::current_exe().unwrap();
    let mut differences = BTreeMap::new();
    let mut found: BTreeMap<String, usize> = BTreeMap::new();
    let mut seconds: BTreeMap<String, f64> = BTreeMap::new();
    let mut runs = Vec::new();
    let maps_and_races: Vec<(PathBuf, Race)> = openbw_host::parity_maps(MAPS)
        .into_iter()
        .flat_map(|m| RACES.map(|r| (m.clone(), r)))
        .collect();
    let outputs = openbw_host::parallel(&maps_and_races, |(map, race)| {
        Command::new(&exe)
            .args(["one_map", "--exact", "--ignored", "--nocapture"])
            .env(MAP_VAR, map)
            .env(RACE_VAR, race.id().to_string())
            .output()
            .unwrap()
    });
    let mut checked = Vec::new();
    for ((map, race), out) in maps_and_races.iter().zip(outputs) {
        let name = openbw_host::map_name(map);
        let stdout = String::from_utf8_lossy(&out.stdout);
        checked.extend(property::read_report(&stdout));
        for line in stdout
            .lines()
            .filter_map(|l| l.strip_prefix("PARITY_FOUND "))
        {
            if let Some((kind, count)) = line.split_once(": ") {
                *found.entry(format!("{kind} as {race:?}")).or_default() +=
                    count.parse::<usize>().unwrap_or(0);
            }
        }
        let mut run = (0.0, 0.0);
        for line in stdout
            .lines()
            .filter_map(|l| l.strip_prefix("PARITY_SECONDS "))
        {
            if let Some((phase, s)) = line.split_once(": ") {
                let s: f64 = s.parse().unwrap_or(0.0);
                *seconds.entry(phase.to_owned()).or_default() += s;
                if phase.starts_with("C++") {
                    run.0 += s;
                } else {
                    run.1 += s;
                }
            }
        }
        runs.push((run.0, run.1, format!("{name} as {race:?}")));
        let mut lines: Vec<String> = stdout
            .lines()
            .filter_map(|l| l.strip_prefix("PARITY "))
            .map(str::to_owned)
            .collect();
        if !stdout.lines().any(|l| l == "PARITY_DONE") {
            let stderr = String::from_utf8_lossy(&out.stderr);
            lines.push(format!(
                "the child failed: {}",
                stderr.lines().last().unwrap_or("")
            ));
        }
        if !lines.is_empty() {
            differences.insert(format!("{name} as {race:?}"), lines);
        }
    }
    println!("seconds by phase, all runs:");
    for (phase, s) in &seconds {
        println!("  {s:9.2}  {phase}");
    }
    runs.sort_by(|a, b| (b.0 + b.1).total_cmp(&(a.0 + a.1)));
    println!("slowest runs (C++, port):");
    for (cpp, port, run) in runs.iter().take(10) {
        println!("  {cpp:7.2} {port:7.2}  {run}");
    }
    for line in property::diagnostics(&checked) {
        println!("{line}");
    }
    let data = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../sdk/tests/data");
    let failures = property::judge(&checked, &property::known_file(&data));
    if !failures.is_empty() {
        differences.insert("properties of C++".to_owned(), failures);
    }
    assert!(differences.is_empty(), "{differences:#?}");
    // Every race finds blocks, defenses and walls; Zerg sim cities and Terran
    // ComSat slots; Protoss neither slot.
    let expected = RACES.iter().flat_map(|&race| {
        let slots = match race {
            Race::ZERG => ["small slots", "medium slots"].as_slice(),
            Race::TERRAN => ["small slots"].as_slice(),
            _ => [].as_slice(),
        };
        ["blocks", "defenses", "walls"]
            .iter()
            .chain(slots)
            .map(move |kind| format!("{kind} as {race:?}"))
    });
    for kind in expected {
        assert!(
            found.get(&kind).is_some_and(|&n| n > 0),
            "C++ found no {kind}: {found:#?}"
        );
    }
}

struct Port<'a> {
    input: &'a MapInput,
    bwem: &'a Map,
    layout: &'a bweb::Layout,
    /// The layout after `onStart`, the stations, the wall and the blocks.
    snapshots: [&'a bweb::Layout; 4],
    stations: &'a Stations,
    blocks: &'a Blocks,
    walls: &'a Walls,
    walk: Vec<UnitType>,
    place: [UnitType; 6],
}

impl Port<'_> {
    /// The choke points by index.
    fn choke_points(&self) -> Vec<ChokePoint<'_>> {
        let mut cps: Vec<_> = self.bwem.areas().flat_map(|a| a.choke_points()).collect();
        cps.sort_by_key(|cp| cp.id());
        cps.dedup_by_key(|cp| cp.id());
        cps
    }

    /// Base centers, start locations, choke point centers, the center of the
    /// map and a point outside it.
    fn points(&self) -> Vec<(i32, i32)> {
        let bases = self
            .bwem
            .areas()
            .flat_map(|a| a.bases())
            .map(|b| b.center());
        let starts = self
            .bwem
            .starting_locations()
            .iter()
            .map(|&t| Position::from(t));
        let chokes = self
            .choke_points()
            .into_iter()
            .map(|cp| Position::from(cp.center()));
        bases
            .chain(starts)
            .chain(chokes)
            .chain([self.bwem.center(), Position::new(-1, -1)])
            .map(|p| (p.x, p.y))
            .collect()
    }

    /// The locations of the first bases.
    fn path_tiles(&self) -> Vec<(i32, i32)> {
        self.bwem
            .areas()
            .flat_map(|a| a.bases())
            .take(PATH_BASES)
            .map(|b| (b.location().x, b.location().y))
            .collect()
    }

    /// The searches of `Path` in the order of `bweb_reference::analyse`, in the
    /// modes from `first_mode` on, with one cache of unreachable tiles.
    fn compare_searches(
        &self,
        tiles: &[(i32, i32)],
        first_mode: i32,
        r: &Reference,
    ) -> Vec<String> {
        let layout = self.layout;
        let mut pathfinding = Pathfinding::new(layout);
        let cost = |_: &Path, t: TilePosition| f64::from((t.x * 7 + t.y * 13) % 5) * 8.0;
        let terrain = |path: &Path, t: TilePosition| path.is_terrain_walkable(layout, t);
        let unit = |path: &Path, t: TilePosition| path.is_unit_walkable(layout, t);
        let tiles: Vec<TilePosition> = tiles
            .iter()
            .map(|&(x, y)| TilePosition::new(x, y))
            .collect();

        let mut ours = Vec::new();
        for ty in PATH_TYPES {
            for &source in &tiles {
                for &target in &tiles {
                    for mode in first_mode..=UNIT_MODE {
                        let mut path = Path::new(source, target, ty);
                        match mode {
                            0 => path.astar(&mut pathfinding, layout, terrain),
                            1 => path.astar_with_cost(&mut pathfinding, layout, cost, terrain),
                            2 => path.jps(&mut pathfinding, layout, terrain),
                            _ => path.jps(&mut pathfinding, layout, unit),
                        }
                        ours.push(SearchRecord {
                            reachable: path.is_reachable(),
                            distance: path.distance(),
                            tiles: path.tiles().iter().map(|t| (t.x, t.y)).collect(),
                        });
                    }
                }
            }
        }
        let mut out = Vec::new();
        list("searches", &ours, &r.searches, &mut out);
        out
    }

    /// The differences from `r`; the seconds of each phase go to `seconds`.
    fn compare(
        &self,
        points: &[(i32, i32)],
        r: &Reference,
        seconds: &mut Vec<(String, f64)>,
    ) -> Vec<String> {
        let mut lap = Instant::now();
        let mut time = |phase: &str| {
            seconds.push((format!("port {phase}"), lap.elapsed().as_secs_f64()));
            lap = Instant::now();
        };
        let mut out = Vec::new();
        let size = self.bwem.size();
        if r.size != (size.x, size.y) {
            out.push(format!("size: port {size:?}, C++ {:?}", r.size));
            return out;
        }
        let tiles: Vec<TilePosition> = (0..size.y)
            .flat_map(|y| (0..size.x).map(move |x| TilePosition::new(x, y)))
            .collect();

        for (k, &ty) in self.walk.iter().enumerate() {
            let ours: Vec<bool> = tiles
                .iter()
                .map(|&t| self.layout.is_walkable(t, ty))
                .collect();
            cells(
                &format!("walkable {}", ty.id()),
                &tiles,
                &ours,
                &r.walkable[k],
                |_| false,
                &mut out,
            );
        }
        // Where buildings of several types overlap, the type is that of the last
        // in the order of `getNeutralUnits`, an `unordered_set` the port does not
        // see; there only whether the tile is used is compared.
        let mut types: Vec<Vec<i32>> = vec![Vec::new(); tiles.len()];
        for n in self
            .input
            .neutrals
            .iter()
            .filter(|n| n.unit_type.is_building())
        {
            let (at, extent, type_id) = (n.initial_tile_position, n.size(), n.unit_type.id());
            for y in at.y..at.y + extent.y {
                for x in at.x..at.x + extent.x {
                    if (0..size.x).contains(&x) && (0..size.y).contains(&y) {
                        let cell = &mut types[(y * size.x + x) as usize];
                        if !cell.contains(&type_id) {
                            cell.push(type_id);
                        }
                    }
                }
            }
        }
        let used: Vec<Option<i32>> = tiles
            .iter()
            .map(|&t| self.layout.used_by(t, 1, 1).map(UnitType::id))
            .collect();
        let theirs: Vec<Option<i32>> = r
            .used
            .iter()
            .zip(&used)
            .zip(&types)
            .map(|((&theirs, &ours), types)| match (theirs, ours) {
                (Some(a), Some(b))
                    if types.len() > 1 && types.contains(&a) && types.contains(&b) =>
                {
                    ours
                }
                _ => theirs,
            })
            .collect();
        cells("used", &tiles, &used, &theirs, |_| false, &mut out);
        let reserved: Vec<bool> = tiles
            .iter()
            .map(|&t| self.layout.is_reserved(t, 1, 1))
            .collect();
        cells(
            "reserved",
            &tiles,
            &reserved,
            &r.reserved,
            |_| false,
            &mut out,
        );
        // The layout after `onStart`, the stations, the wall and the blocks.
        if r.snapshots.len() != self.snapshots.len() {
            out.push(format!("snapshots: C++ {}", r.snapshots.len()));
        }
        for (stage, (ours, theirs)) in self.snapshots.iter().zip(&r.snapshots).enumerate() {
            let used: Vec<Option<i32>> = tiles
                .iter()
                .map(|&t| ours.used_by(t, 1, 1).map(UnitType::id))
                .collect();
            let theirs_used: Vec<Option<i32>> = theirs
                .used
                .iter()
                .zip(&used)
                .zip(&types)
                .map(|((&theirs, &ours), types)| match (theirs, ours) {
                    (Some(a), Some(b))
                        if types.len() > 1 && types.contains(&a) && types.contains(&b) =>
                    {
                        ours
                    }
                    _ => theirs,
                })
                .collect();
            let name = format!("used at stage {stage}");
            cells(&name, &tiles, &used, &theirs_used, |_| false, &mut out);
            let reserved: Vec<bool> = tiles.iter().map(|&t| ours.is_reserved(t, 1, 1)).collect();
            let name = format!("reserved at stage {stage}");
            cells(
                &name,
                &tiles,
                &reserved,
                &theirs.reserved,
                |_| false,
                &mut out,
            );
        }
        time("walkable");
        for (k, &ty) in self.place.iter().enumerate() {
            let ours: Vec<bool> = tiles
                .iter()
                .map(|&t| self.layout.can_place(self.bwem, ty, t))
                .collect();
            let occupied = &r.occupied[k];
            let depot = ty.is_resource_depot();
            let skip = |t: TilePosition| depot && occupied[(t.y * size.x + t.x) as usize];
            cells(
                &format!("placeable {}", ty.id()),
                &tiles,
                &ours,
                &r.placeable[k],
                skip,
                &mut out,
            );
        }
        time("placeable");

        let positions: Vec<Position> = points.iter().map(|&(x, y)| Position::new(x, y)).collect();
        let mut distance_diffs = Vec::new();
        for (i, &a) in positions.iter().enumerate() {
            for (j, &b) in positions.iter().enumerate() {
                let theirs = r.ground_distances[i * positions.len() + j];
                let ours = self
                    .layout
                    .ground_distance_via_choke_points(self.bwem, a, b);
                // With no path, BWEB gives the straight distance when a position
                // is in no area; the port gives none.
                let no_path = self.bwem.choke_point_path(a, b).is_none() && theirs == a.distance(b);
                if ours.unwrap_or(f64::MAX) != theirs && !(ours.is_none() && no_path) {
                    distance_diffs.push(format!("{a:?} -> {b:?}: port {ours:?}, C++ {theirs}"));
                }
            }
        }
        if !distance_diffs.is_empty() {
            out.push(format!(
                "ground distances: {} of {} differ, e.g. {:?}",
                distance_diffs.len(),
                r.ground_distances.len(),
                &distance_diffs[..distance_diffs.len().min(3)]
            ));
        }

        let paths: Vec<Vec<i32>> = positions
            .iter()
            .flat_map(|&a| {
                // An unreachable point has an empty path.
                positions.iter().map(move |&b| {
                    self.bwem
                        .choke_point_path(a, b)
                        .map(|p| p.iter().map(|cp| cp.id().index() as i32).collect())
                        .unwrap_or_default()
                })
            })
            .collect();
        list("choke point paths", &paths, &r.paths, &mut out);

        let cps = self.choke_points();
        let ours: Vec<(i32, Vec<(i32, i32)>)> = cps
            .iter()
            .map(|&cp| {
                let tiles = positions
                    .iter()
                    .map(|&p| {
                        let t = self
                            .layout
                            .closest_choke_point_tile(cp, p)
                            .unwrap_or(Position::new(32000, 32000));
                        (t.x, t.y)
                    })
                    .collect();
                (cp.id().index() as i32, tiles)
            })
            .collect();
        let mut theirs = r.closest_choke_tiles.clone();
        theirs.sort_by_key(|(index, _)| *index);
        list("closest choke tiles", &ours, &theirs, &mut out);
        time("choke point queries");

        // `TilePositions::Invalid` for none.
        let tile = |t: Option<TilePosition>| t.map_or((1000, 1000), |t| (t.x, t.y));
        let station = |s: &bweb::Station| {
            let base = self.bwem.base(s.base());
            let centroid = s.resource_centroid();
            (
                (base.location().x, base.location().y),
                s.is_main(),
                s.is_natural(),
                s.choke_point().map(|cp| cp.index() as i32),
                (centroid.x, centroid.y),
                (tile(s.small_slot()), tile(s.medium_slot()), tile(None)),
                s.defenses()
                    .iter()
                    .map(|&d| tile(Some(d)))
                    .collect::<Vec<_>>(),
            )
        };
        let ours: Vec<_> = self.stations.iter().map(station).collect();
        let theirs: Vec<_> = r
            .stations
            .iter()
            .map(|s: &StationRecord| {
                (
                    s.location,
                    s.main,
                    s.natural,
                    s.choke,
                    s.resource_centroid,
                    (s.small, s.medium, s.pocket),
                    s.defenses.clone(),
                )
            })
            .collect();
        list("stations", &ours, &theirs, &mut out);

        let xy = |t: &TilePosition| (t.x, t.y);
        let ours: Vec<BlockRecord> = self
            .blocks
            .iter()
            .map(|b| BlockRecord {
                tile: xy(&b.tile()),
                size: (b.width(), b.height()),
                center: (b.center().x, b.center().y),
                proxy: b.is_proxy(),
                defensive: b.is_defensive(),
                kind: b.kind() as i32,
                slots: [
                    b.small_tiles(),
                    b.medium_tiles(),
                    b.large_tiles(),
                    b.wide_tiles(),
                ]
                .map(|tiles| tiles.iter().map(xy).collect()),
            })
            .collect();
        list("blocks", &ours, &r.blocks, &mut out);

        // The defense angle within a few ulps, as for stations.
        let ours = self.walls.iter().next().map(|w| WallRecord {
            defense_angle: w.defense_angle(),
            choke: Some(w.choke_point().index() as i32),
            area: Some(i32::from(w.area().get())),
            pieces: [
                w.small_tiles(),
                w.medium_tiles(),
                w.large_tiles(),
                w.openings(),
            ]
            .map(|tiles| tiles.iter().map(xy).collect()),
            defenses: (0..=3)
                .filter(|&row| !w.defenses(row).is_empty())
                .map(|row| (row, w.defenses(row).iter().map(xy).collect()))
                .collect(),
        });
        let same = match (&ours, &r.wall) {
            (Some(a), Some(b)) => {
                a.pieces == b.pieces
                    && (a.choke, a.area) == (b.choke, b.area)
                    && a.defenses == b.defenses
                    && (a.defense_angle - b.defense_angle).abs() <= 1e-12
            }
            (a, b) => a.is_none() && b.is_none(),
        };
        if !same || self.walls.iter().count() > 1 {
            out.push(format!("wall: port {ours:?}, C++ {:?}", r.wall));
        }
        // Within a few ulps: `atan2` of the port and of glibc differ in the last
        // bit, and an angle may sum two of them.
        let angles: Vec<String> = self
            .stations
            .iter()
            .zip(&r.stations)
            .filter(|(s, c)| (s.defense_angle() - c.defense_angle).abs() > 1e-12)
            .map(|(s, c)| format!("port {}, C++ {}", s.defense_angle(), c.defense_angle))
            .collect();
        if !angles.is_empty() {
            out.push(format!(
                "defense angles: {} differ, e.g. {:?}",
                angles.len(),
                &angles[..angles.len().min(3)]
            ));
        }

        let location = |s: Option<&bweb::Station>| {
            s.map(|s| {
                let t = self.bwem.base(s.base()).location();
                (t.x, t.y)
            })
        };
        let ours = (
            location(self.stations.own_main()),
            location(self.stations.own_natural()),
        );
        if ours != (r.starting_main, r.starting_natural) {
            out.push(format!(
                "starting main, natural: port {ours:?}, C++ {:?}",
                (r.starting_main, r.starting_natural)
            ));
        }
        time("comparison of stations, blocks and walls");
        out
    }
}

/// The tiles where the port and C++ differ, but for those `skip` leaves out.
fn cells<T: PartialEq + std::fmt::Debug>(
    name: &str,
    tiles: &[TilePosition],
    ours: &[T],
    theirs: &[T],
    skip: impl Fn(TilePosition) -> bool,
    out: &mut Vec<String>,
) {
    if ours.len() != theirs.len() {
        out.push(format!("{name}: port {}, C++ {}", ours.len(), theirs.len()));
        return;
    }
    let diffs: Vec<String> = tiles
        .iter()
        .zip(ours.iter().zip(theirs))
        .filter(|&(&t, (a, b))| a != b && !skip(t))
        .map(|(t, (a, b))| format!("({}, {}): port {a:?}, C++ {b:?}", t.x, t.y))
        .collect();
    if !diffs.is_empty() {
        out.push(format!(
            "{name}: {} of {} differ, e.g. {:?}",
            diffs.len(),
            theirs.len(),
            &diffs[..diffs.len().min(3)]
        ));
    }
}

fn list<T: PartialEq + std::fmt::Debug>(
    name: &str,
    ours: &[T],
    theirs: &[T],
    out: &mut Vec<String>,
) {
    if ours.len() != theirs.len() {
        out.push(format!("{name}: port {}, C++ {}", ours.len(), theirs.len()));
    }
    let diffs: Vec<String> = ours
        .iter()
        .zip(theirs)
        .filter(|(a, b)| a != b)
        .map(|(a, b)| format!("port {a:?}, C++ {b:?}"))
        .collect();
    if !diffs.is_empty() {
        out.push(format!(
            "{name}: {} differ, e.g. {:?}",
            diffs.len(),
            &diffs[..diffs.len().min(2)]
        ));
    }
}
