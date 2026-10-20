//! BWEB on the fixtures of real maps against its answers, next to each
//! fixture: `bweb.txt` for what does not depend on the race, computed as
//! Terran, and `bweb.<race>.txt` for the layout of each race. `openbw-host`'s
//! `bweb_parity` compares the same with C++ BWEB.
//!
//! Stations, blocks and walls in full; grids of tiles as the count of their
//! set tiles and a hash; ground distances, choke point paths, closest choke
//! tiles and searches as hashes, a line per row. Placement is
//! `Layout::can_place`, which for a resource depot leaves out the units and
//! creep `canBuildHere` checks; the type of a used tile under neutrals of
//! several types is that of the last in the order of the fixture; with no path
//! between points the ground distance is none, where BWEB gives the straight
//! one for a point in no area. There `bweb_parity` allows the port and C++ to
//! differ.

mod answer;
mod fixture;

use std::collections::BTreeMap;
use std::fmt::Write as _;

use bwapi::{Position, Race, TilePosition, UnitType};
use bwapi_sdk::adapter::footprint;
use bwapi_sdk::bweb::{self, Blocks, Path, Pathfinding, Stations, WallSpec, Walls};
use bwapi_sdk::bwem::{ChokePoint, Map};
use fixture::{Fixture, Start};

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

const RACES: [(Race, &str); 3] = [
    (Race::TERRAN, "terran"),
    (Race::ZERG, "zerg"),
    (Race::PROTOSS, "protoss"),
];

/// The mode of the searches over `unitWalkable`, the only one that sees the
/// buildings the race places; the modes before it search the terrain: 0 A*,
/// 1 A* with a cost, 2 JPS.
const UNIT_MODE: i32 = 3;

/// `name`, the count of the tiles where `set` holds and the hash of them all.
fn grid_line(out: &mut String, name: &str, size: TilePosition, set: impl Fn(TilePosition) -> i64) {
    let values: Vec<i64> = (0..size.y)
        .flat_map(|y| (0..size.x).map(move |x| TilePosition::new(x, y)))
        .map(set)
        .collect();
    let count = values.iter().filter(|&&v| v != 0).count();
    writeln!(out, "{name} {count} {}", answer::hash(values)).unwrap();
}

fn tile(t: TilePosition) -> String {
    format!("{},{}", t.x, t.y)
}

fn tiles<'a>(ts: impl IntoIterator<Item = &'a TilePosition>) -> String {
    ts.into_iter()
        .map(|&t| tile(t))
        .collect::<Vec<_>>()
        .join(" ")
}

/// The choke points by index.
fn choke_points(bwem: &Map) -> Vec<ChokePoint<'_>> {
    let mut cps: Vec<_> = bwem.areas().flat_map(|a| a.choke_points()).collect();
    cps.sort_by_key(|cp| cp.id());
    cps.dedup_by_key(|cp| cp.id());
    cps
}

/// The answers of one race: what does not depend on the race, if `shared`,
/// and the layout of the race.
fn race_answers(
    fixture: &Fixture,
    start: &Start,
    shared: bool,
) -> Result<(String, String), String> {
    let input = fixture.map_input(start);
    let mut bwem = Map::initialize(&input).map_err(|e| format!("refused: {e}"))?;
    bwem.find_bases_for_starting_locations();
    let race = start.race;
    let mut layout = bweb::Layout::new(&bwem, &input);
    let after_start = layout.clone();
    let stations = Stations::analyze(&bwem, &mut layout, start.start_location, race);
    let after_stations = layout.clone();
    let mut walls = Walls::new(&bwem);
    let enemy = start.enemy.unwrap_or(Race::NONE);
    let _ = WallSpec::natural(race, enemy, &bwem, &stations)
        .and_then(|spec| walls.build(&bwem, &mut layout, &stations, start, &spec));
    let after_wall = layout.clone();
    let blocks = Blocks::find(&bwem, &mut layout, &stations, race, start);
    let size = bwem.size();

    let mut common = String::new();
    let cps = choke_points(&bwem);
    if shared {
        let out = &mut common;
        writeln!(out, "answers bweb 1").unwrap();
        writeln!(out, "map {}", fixture.map).unwrap();
        for ty in WALK_TYPES {
            let fp = footprint(ty);
            let name = format!("walkable {}", ty.id());
            grid_line(out, &name, size, |t| i64::from(layout.is_walkable(t, &fp)));
        }

        // Base centers, start locations, choke point centers, the center of
        // the map and a point outside it.
        let points: Vec<Position> = bwem
            .areas()
            .flat_map(|a| a.bases())
            .map(|b| b.center())
            .chain(bwem.starting_locations().iter().map(|&t| Position::from(t)))
            .chain(cps.iter().map(|cp| Position::from(cp.center())))
            .chain([bwem.center(), Position::new(-1, -1)])
            .collect();
        writeln!(
            out,
            "from {} points: ground distance, choke point path",
            points.len()
        )
        .unwrap();
        for &a in &points {
            let distances = points.iter().map(|&b| {
                layout
                    .ground_distance_via_choke_points(&bwem, a, b)
                    .map_or(-1, |d| d.to_bits() as i64)
            });
            let paths = points.iter().flat_map(|&b| {
                // An unreachable point has an empty path.
                let path: Vec<i64> = bwem
                    .choke_point_path(a, b)
                    .map(|p| p.iter().map(|cp| cp.id().index() as i64).collect())
                    .unwrap_or_default();
                std::iter::once(path.len() as i64).chain(path)
            });
            writeln!(
                out,
                "{},{}: {} {}",
                a.x,
                a.y,
                answer::hash(distances),
                answer::hash(paths)
            )
            .unwrap();
        }
        writeln!(out, "closest choke tiles").unwrap();
        for &cp in &cps {
            let found = points.iter().flat_map(|&p| {
                let t = layout
                    .closest_choke_point_tile(cp, p)
                    .unwrap_or(Position::new(32000, 32000));
                [i64::from(t.x), i64::from(t.y)]
            });
            writeln!(out, "{}: {}", cp.id().index(), answer::hash(found)).unwrap();
        }
    }

    let mut own = String::new();
    {
        let out = &mut own;
        writeln!(out, "answers bweb 1").unwrap();
        writeln!(out, "map {}", fixture.map).unwrap();
        writeln!(out, "race {}", race.id()).unwrap();
        grid_line(out, "used", size, |t| {
            i64::from(layout.used_by(t, 1, 1).unwrap_or(-1) + 1)
        });
        grid_line(out, "reserved", size, |t| {
            i64::from(layout.is_reserved(t, 1, 1))
        });
        // The layout after `onStart`, the stations and the wall; the lines
        // above are after the blocks.
        let snapshots = [&after_start, &after_stations, &after_wall];
        for (stage, snapshot) in snapshots.into_iter().enumerate() {
            grid_line(out, &format!("used at stage {stage}"), size, |t| {
                i64::from(snapshot.used_by(t, 1, 1).unwrap_or(-1) + 1)
            });
            grid_line(out, &format!("reserved at stage {stage}"), size, |t| {
                i64::from(snapshot.is_reserved(t, 1, 1))
            });
        }
        for ty in PLACE_TYPES {
            let fp = footprint(ty);
            let name = format!("placeable {}", ty.id());
            grid_line(out, &name, size, |t| {
                i64::from(layout.can_place(&bwem, &fp, t))
            });
        }

        let location = |s: Option<&bweb::Station>| {
            s.map_or("-".to_owned(), |s| tile(bwem.base(s.base()).location()))
        };
        writeln!(out, "starting_main {}", location(stations.own_main())).unwrap();
        writeln!(out, "starting_natural {}", location(stations.own_natural())).unwrap();
        let slot = |t: Option<TilePosition>| t.map_or("-".to_owned(), tile);
        writeln!(out, "stations {}", stations.iter().len()).unwrap();
        for s in &stations {
            let base = bwem.base(s.base());
            let centroid = s.resource_centroid();
            writeln!(
                out,
                "{}{}{} choke {} centroid {},{} small {} medium {} angle {:.9} defenses {} \
                 angle_bits {:016x}",
                tile(base.location()),
                if s.is_main() { " main" } else { "" },
                if s.is_natural() { " natural" } else { "" },
                s.choke_point()
                    .map_or("-".to_owned(), |cp| cp.index().to_string()),
                centroid.x,
                centroid.y,
                slot(s.small_slot()),
                slot(s.medium_slot()),
                s.defense_angle(),
                tiles(s.defenses()),
                s.defense_angle().to_bits()
            )
            .unwrap();
        }

        writeln!(out, "blocks {}", blocks.iter().count()).unwrap();
        for b in &blocks {
            writeln!(
                out,
                "{} size {} {} center {},{}{}{} small {} | medium {} | large {} | wide {}",
                tile(b.tile()),
                b.width(),
                b.height(),
                b.center().x,
                b.center().y,
                if b.is_proxy() { " proxy" } else { "" },
                if b.is_defensive() { " defensive" } else { "" },
                tiles(b.small_tiles()),
                tiles(b.medium_tiles()),
                tiles(b.large_tiles()),
                tiles(b.wide_tiles())
            )
            .unwrap();
        }

        let all: Vec<_> = walls.iter().collect();
        writeln!(out, "walls {}", all.len()).unwrap();
        for w in all {
            let defenses: Vec<String> = (0..=3)
                .filter(|&row| !w.defenses(row).is_empty())
                .map(|row| format!("{row}: {}", tiles(w.defenses(row))))
                .collect();
            writeln!(
                out,
                "angle {:.9} small {} | medium {} | large {} | openings {} | defenses {} | \
                 choke {} area {} angle_bits {:016x}",
                w.defense_angle(),
                tiles(w.small_tiles()),
                tiles(w.medium_tiles()),
                tiles(w.large_tiles()),
                tiles(w.openings()),
                defenses.join(" / "),
                w.choke_point().index(),
                w.area().get(),
                w.defense_angle().to_bits()
            )
            .unwrap();
        }
    }

    // The searches between the first bases with one cache of unreachable
    // tiles, in the order `bweb_parity` runs them: the terrain modes only for
    // what does not depend on the race.
    let path_tiles: Vec<TilePosition> = bwem
        .areas()
        .flat_map(|a| a.bases())
        .take(PATH_BASES)
        .map(|b| b.location())
        .collect();
    let first_mode = if shared { 0 } else { UNIT_MODE };
    let mut pathfinding = Pathfinding::new(&layout);
    let cost = |_: &Path, t: TilePosition| f64::from((t.x * 7 + t.y * 13) % 5) * 8.0;
    let terrain = |path: &Path, t: TilePosition| path.is_terrain_walkable(&layout, t);
    let unit = |path: &Path, t: TilePosition| path.is_unit_walkable(&layout, t);
    let mut searches: BTreeMap<(i32, i32, usize), Vec<i64>> = BTreeMap::new();
    for ty in PATH_TYPES {
        for (i, &source) in path_tiles.iter().enumerate() {
            for &target in &path_tiles {
                for mode in first_mode..=UNIT_MODE {
                    let mut path = Path::new(source, target, footprint(ty));
                    match mode {
                        0 => path.astar(&mut pathfinding, &layout, terrain),
                        1 => path.astar_with_cost(&mut pathfinding, &layout, cost, terrain),
                        2 => path.jps(&mut pathfinding, &layout, terrain),
                        _ => path.jps(&mut pathfinding, &layout, unit),
                    }
                    let row = searches.entry((ty.id(), mode, i)).or_default();
                    row.push(i64::from(path.is_reachable()));
                    row.push(path.distance().to_bits() as i64);
                    row.push(path.tiles().len() as i64);
                    row.extend(
                        path.tiles()
                            .iter()
                            .flat_map(|t| [i64::from(t.x), i64::from(t.y)]),
                    );
                }
            }
        }
    }
    writeln!(common, "searches").unwrap();
    writeln!(own, "searches").unwrap();
    for ((ty, mode, i), values) in searches {
        let out = if mode == UNIT_MODE {
            &mut own
        } else {
            &mut common
        };
        let line = format!(
            "{ty} mode {mode} from {}: {}",
            tile(path_tiles[i]),
            answer::hash(values)
        );
        writeln!(out, "{line}").unwrap();
    }
    Ok((common, own))
}

#[test]
fn bweb_gives_its_answers() {
    let fixtures = answer::fixtures();
    let jobs: Vec<(usize, usize)> = (0..fixtures.len())
        .flat_map(|m| (0..RACES.len()).map(move |r| (m, r)))
        .collect();
    let read = answer::parallel(&fixtures, |path| {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Fixture::parse(&text).map_err(|e| format!("{}: {e}", path.display()))
    });
    let results = answer::parallel(&jobs, |&(m, r)| {
        let fixture = read[m].as_ref().map_err(Clone::clone)?;
        let (race, name) = RACES[r];
        let start = fixture
            .start(race)
            .ok_or_else(|| format!("no start as {name}"))?;
        let (common, own) = race_answers(fixture, start, r == 0)?;
        let dir = &fixtures[m];
        if r == 0 {
            answer::check(&dir.with_file_name("bweb.txt"), &common)?;
        }
        answer::check(&dir.with_file_name(format!("bweb.{name}.txt")), &own)
    });
    let mut problems: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (&(m, _), r) in jobs.iter().zip(results) {
        if let Err(e) = r {
            problems
                .entry(fixtures[m].display().to_string())
                .or_default()
                .push(e);
        }
    }
    answer::assert_none(problems);
}
