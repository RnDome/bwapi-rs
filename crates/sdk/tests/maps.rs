//! BWEM and BWEB on the fixtures of real maps. Each map is analysed once as each
//! race, and what the analysis and BWEB give is checked three ways:
//!
//! - against the answers next to each fixture: `bwem.txt` for BWEM as Terran,
//!   then after each blocking neutral and a few minerals are destroyed one by
//!   one; `bweb.txt` for what BWEB gives regardless of the race, as Terran, and
//!   `bweb.<race>.txt` for the layout of each race;
//! - for the properties of `tests/property`, with the known violations;
//! - against the numbers of the tests of BWEM-community, on their maps.
//!
//! BWEM: the analysis in full, but for the mini tiles: their altitudes and
//! areas are hashes, and so are the distances and paths between choke points,
//! a line per choke point. After a destruction every part is a hash, with the
//! length of each list.
//!
//! BWEB: stations, blocks and walls in full; grids of tiles as the count of
//! their set tiles and a hash; ground distances, choke point paths, closest
//! choke tiles and searches as hashes, a line per row. Placement is
//! `Layout::can_place`, which for a resource depot leaves out the units and
//! creep `canBuildHere` checks; the type of a used tile under neutrals of
//! several types is that of the last in the order of the fixture; with no path
//! between points the ground distance is none, where BWEB gives the straight
//! one for a point in no area. There the port and C++ differ.

mod answer;
mod extract;
mod fixture;
mod property;
mod scenario;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::PathBuf;

use bwapi::{Position, Race, TilePosition, UnitType, WalkPosition};
use bwapi_sdk::bweb::{self, Blocks, Stations, WallSpec, Walls};
use bwapi_sdk::bwem::{Map, MapInput, Node};
use bwapi_sdk::grid::Grid;
use extract::{PATH_TYPES, UNIT_MODE, WALK_TYPES};
use fixture::{Fixture, Start};
use property::bweb::Navigation;
use property::bwem::{Terrain, When};
use property::edt;
use property::{Case, Side, Violation};

const RACES: [(Race, &str); 3] = [
    (Race::TERRAN, "terran"),
    (Race::ZERG, "zerg"),
    (Race::PROTOSS, "protoss"),
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

/// The maps of the tests of BWEM-community: size in tiles and the greatest
/// altitude BWEM-community 9a63141 computes on OpenBW. The altitudes of its own
/// test data were taken in 2018, before `LoadData` made fully walkable tiles
/// walkable (2019); ten of them no longer hold for the C++ code either. Those
/// data are in the comments.
const SSCAIT: [(&str, (i32, i32), i16); 15] = [
    ("(2)Benzene.scx", (128, 112), 488),
    ("(2)Destination.scx", (96, 128), 485),      // 2018: 483
    ("(2)Heartbreak Ridge.scx", (128, 96), 455), // 2018: 454
    ("(3)Neo Moon Glaive.scx", (128, 128), 431), // 2018: 429
    ("(3)Tau Cross.scx", (128, 128), 478),
    ("(4)Andromeda.scx", (128, 128), 396),         // 2018: 392
    ("(4)Circuit Breaker.scx", (128, 128), 664),   // 2018: 659
    ("(4)Electric Circuit.scx", (128, 128), 603),  // 2018: 347
    ("(4)Empire of the Sun.scm", (128, 128), 445), // 2018: 444
    ("(4)Fighting Spirit.scx", (128, 128), 493),
    ("(4)Icarus.scm", (128, 128), 463), // 2018: 458
    ("(4)Jade.scx", (128, 128), 640),
    ("(4)La Mancha1.1.scx", (128, 128), 424), // 2018: 408
    ("(4)Python.scx", (128, 128), 1036),      // 2018: 1035
    ("(4)Roadrunner.scx", (128, 128), 416),
];

/// The map of the tests of BWEM-community with its own numbers.
const SHOWDOWN: &str = "scai/(2)Showdown.scx";

/// The violations of the properties, by case.
type Cases = Vec<(Case, Vec<Violation>)>;

/// What a map gives: the answers computed, by file; the cases of the
/// properties; whether it is a map of BWEM-community, and how it differs from
/// its numbers.
#[derive(Default)]
struct Checked {
    answers: Vec<(PathBuf, String)>,
    cases: Cases,
    community: Option<(String, Vec<String>)>,
}

#[test]
fn real_maps_give_their_answers_and_have_their_properties() {
    let fixtures = answer::fixtures();
    let read = answer::parallel(&fixtures, |path| {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Fixture::parse(&text).map_err(|e| format!("{}: {e}", path.display()))
    });
    let jobs: Vec<(usize, usize)> = (0..fixtures.len())
        .flat_map(|m| (0..RACES.len()).map(move |r| (m, r)))
        .collect();
    let results = answer::parallel(&jobs, |&(m, r)| {
        let fixture = read[m].as_ref().map_err(Clone::clone)?;
        check(fixture, &fixtures[m], r)
    });

    let mut problems: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut cases = Vec::new();
    let mut community = BTreeSet::new();
    for (&(m, _), result) in jobs.iter().zip(results) {
        let path = &fixtures[m];
        let mut errors = Vec::new();
        match result {
            Ok(checked) => {
                for (file, text) in &checked.answers {
                    if let Err(e) = answer::check(file, text) {
                        errors.push(e);
                    }
                }
                cases.extend(checked.cases);
                if let Some((map, mismatches)) = checked.community {
                    community.insert(map);
                    errors.extend(mismatches);
                }
            }
            Err(e) => errors.push(e),
        }
        if !errors.is_empty() {
            problems
                .entry(path.display().to_string())
                .or_default()
                .extend(errors);
        }
    }

    // The maps of BWEM-community are among the fixtures checked by default.
    let expected = SSCAIT
        .iter()
        .map(|(name, _, _)| format!("scai/{name}"))
        .chain([SHOWDOWN.to_owned()]);
    let missing: Vec<String> = expected.filter(|m| !community.contains(m)).collect();
    if !missing.is_empty() {
        problems.insert(
            "maps of BWEM-community".to_owned(),
            vec![format!("not checked: {missing:?}")],
        );
    }

    for line in property::diagnostics(&cases) {
        println!("{line}");
    }
    let failures = property::judge(&cases, &property::known_file(&answer::data()));
    if !failures.is_empty() {
        problems.insert("properties".to_owned(), failures);
    }
    answer::assert_none(problems);
}

/// The map of `fixture`, at `path`, as the race of `RACES[r]`: the analysis of
/// BWEM and what BWEB makes of it; as Terran, also BWEM after the destructions
/// and the numbers of BWEM-community.
fn check(fixture: &Fixture, path: &std::path::Path, r: usize) -> Result<Checked, String> {
    let name = fixture
        .map
        .rsplit_once('.')
        .map_or(&*fixture.map, |(n, _)| n);
    let (race, race_name) = RACES[r];
    let start = fixture
        .start(race)
        .ok_or_else(|| format!("no start as {race_name}"))?;
    let input = fixture.map_input(start);
    let mut map = Map::initialize(&input).map_err(|e| format!("refused: {e}"))?;
    let found = map.find_bases_for_starting_locations();

    // What does not depend on the race is answered and searched as Terran
    // only.
    let shared = race == Race::TERRAN;
    let (common, own, case) = bweb(fixture, start, &input, &map, found, name, race_name, shared);
    let mut checked = Checked::default();
    if let Some(common) = common {
        checked
            .answers
            .push((path.with_file_name("bweb.txt"), common));
    }
    checked
        .answers
        .push((path.with_file_name(format!("bweb.{race_name}.txt")), own));
    checked.cases.push(case);

    if shared {
        checked.community = community(&fixture.map, &map, found);
        let (text, cases) = bwem(fixture, &input, &mut map, found, name)?;
        checked
            .answers
            .push((path.with_file_name("bwem.txt"), text));
        checked.cases.extend(cases);
    }
    Ok(checked)
}

/// The answer of BWEM and the cases of its properties: after the analysis,
/// then after each neutral destroyed.
fn bwem(
    fixture: &Fixture,
    input: &MapInput,
    map: &mut Map,
    found: bool,
    name: &str,
) -> Result<(String, Cases), String> {
    let case = |state: String| Case {
        side: Side::Port,
        map: name.to_owned(),
        race: String::new(),
        state,
    };
    let mut out = String::new();
    writeln!(out, "answers bwem 1").unwrap();
    writeln!(out, "map {}", fixture.map).unwrap();
    bwem_state(&mut out, map, found, true);
    let mut cases = vec![(
        case("analysis".into()),
        property::bwem::check(
            &extract::analysis(map, found),
            &Terrain::new(input, &[]),
            When::Analysis,
        ),
    )];
    let mut destroyed = Vec::new();
    for id in scenario::neutrals_to_destroy(map) {
        if map.remove_neutral(id).is_none() {
            return Err(format!("no neutral of unit {id}"));
        }
        writeln!(out, "destroyed {id}").unwrap();
        bwem_state(&mut out, map, found, false);
        destroyed.push(id);
        cases.push((
            case(format!("without {id}")),
            property::bwem::check(
                &extract::analysis(map, found),
                &Terrain::new(input, &destroyed),
                When::AfterDestruction,
            ),
        ));
    }
    Ok((out, cases))
}

/// The map as it is now; in full, or with grids and matrices as hashes.
fn bwem_state(out: &mut String, map: &Map, found: bool, full: bool) {
    writeln!(out, "max_altitude {}", map.max_altitude()).unwrap();
    writeln!(out, "bases_found {}", u8::from(found)).unwrap();
    let altitudes = map.mini_tiles().map(|m| i64::from(m.altitude()));
    writeln!(out, "mini_tile_altitudes {}", answer::hash(altitudes)).unwrap();
    let areas = map.mini_tiles().map(|m| i64::from(m.area_id()));
    writeln!(out, "mini_tile_areas {}", answer::hash(areas)).unwrap();
    if full {
        let size = map.size();
        let tiles = Grid::from_fn(size.x, size.y, |t: TilePosition| {
            map.tile(t).map_or(0, |t| i32::from(t.area_id()))
        });
        fixture::write_grid(out, "tile_areas", &tiles, |&a| a);
    } else {
        let areas = map.tiles().map(|t| i64::from(t.area_id()));
        writeln!(out, "tile_areas {}", answer::hash(areas)).unwrap();
    }

    // A list in full, or its length and hash.
    let list = |out: &mut String, name: &str, lines: Vec<String>| {
        if full {
            writeln!(out, "{name} {}", lines.len()).unwrap();
            for line in &lines {
                writeln!(out, "{line}").unwrap();
            }
        } else {
            let bytes = lines.iter().flat_map(|l| l.bytes().chain([b'\n']));
            writeln!(
                out,
                "{name} {} {}",
                lines.len(),
                answer::hash(bytes.map(i64::from))
            )
            .unwrap();
        }
    };

    let areas = map.areas().map(|a| {
        format!(
            "{} top {},{} mini_tiles {} group {} box {},{} {},{}",
            a.id().get(),
            a.top().x,
            a.top().y,
            a.size_in_mini_tiles(),
            a.group(),
            a.bounds().0.x,
            a.bounds().0.y,
            a.bounds().1.x,
            a.bounds().1.y
        )
    });
    list(out, "areas", areas.collect());

    let mut cps: Vec<_> = map.areas().flat_map(|a| a.choke_points()).collect();
    cps.sort_by_key(|cp| cp.id());
    cps.dedup_by_key(|cp| cp.id());
    let lines = cps.iter().map(|cp| {
        let (a, b) = cp.areas();
        let xy = |n| {
            let w = cp.node(n);
            format!("{},{}", w.x, w.y)
        };
        format!(
            "{} areas {} {} ends {} {} {}{}{}",
            cp.id().index(),
            a.id().get(),
            b.id().get(),
            xy(Node::End1),
            xy(Node::Middle),
            xy(Node::End2),
            if cp.is_pseudo() { " pseudo" } else { "" },
            if cp.is_blocked() { " blocked" } else { "" }
        )
    });
    list(out, "choke_points", lines.collect());

    let distances = |a: &bwapi_sdk::bwem::ChokePoint| {
        cps.iter()
            .map(|b| i64::from(a.distance_to(b.id()).unwrap_or(-1)))
            .collect::<Vec<_>>()
    };
    // An unreachable choke point has an empty path.
    let path = |a: &bwapi_sdk::bwem::ChokePoint| {
        cps.iter()
            .flat_map(|b| {
                let path: Vec<i64> = a
                    .path_to(b.id())
                    .map(|p| p.iter().map(|cp| cp.id().index() as i64).collect())
                    .unwrap_or_default();
                std::iter::once(path.len() as i64).chain(path)
            })
            .collect::<Vec<_>>()
    };
    if full {
        writeln!(out, "choke_point_distances").unwrap();
        for a in &cps {
            writeln!(out, "{}: {}", a.id().index(), answer::hash(distances(a))).unwrap();
        }
        writeln!(out, "choke_point_paths").unwrap();
        for a in &cps {
            writeln!(out, "{}: {}", a.id().index(), answer::hash(path(a))).unwrap();
        }
    } else {
        let all = cps.iter().flat_map(distances);
        writeln!(out, "choke_point_distances {}", answer::hash(all)).unwrap();
        let all = cps.iter().flat_map(path);
        writeln!(out, "choke_point_paths {}", answer::hash(all)).unwrap();
    }

    let ids = |ids: Vec<i32>| ids.iter().map(i32::to_string).collect::<Vec<_>>().join(" ");
    let bases = map.areas().flat_map(|a| a.bases()).map(|b| {
        let blocking: Vec<i32> = b.blocking_minerals().map(|m| m.unit_id()).collect();
        format!(
            "area {} location {},{}{} minerals {} geysers {}{}",
            b.area().id().get(),
            b.location().x,
            b.location().y,
            if b.is_starting() { " starting" } else { "" },
            ids(b.minerals().map(|m| m.unit_id()).collect()),
            ids(b.geysers().map(|g| g.unit_id()).collect()),
            if blocking.is_empty() {
                String::new()
            } else {
                format!(" blocking {}", ids(blocking))
            }
        )
    });
    list(out, "bases", bases.collect());
}

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

/// BWEB on `bwem` as the race of `start`: the answer of what does not depend
/// on the race, if `shared`; the answer of the layout of the race; the case of
/// its properties.
#[allow(clippy::too_many_arguments)]
fn bweb(
    fixture: &Fixture,
    start: &Start,
    input: &MapInput,
    bwem: &Map,
    found: bool,
    name: &str,
    race_name: &str,
    shared: bool,
) -> (Option<String>, String, (Case, Vec<Violation>)) {
    let race = start.race;
    let mut layout = bweb::Layout::new(bwem, input);
    let after_start = layout.clone();
    let stations = Stations::analyze(bwem, &mut layout, start.start_location, race);
    let after_stations = layout.clone();
    let mut walls = Walls::new(bwem);
    let enemy = start.enemy.unwrap_or(Race::NONE);
    let _ = WallSpec::natural(race, enemy, bwem, &stations)
        .and_then(|spec| walls.build(bwem, &mut layout, &stations, start, &spec));
    let after_wall = layout.clone();
    let blocks = Blocks::find(bwem, &mut layout, &stations, race, start);
    let snapshots = [&after_start, &after_stations, &after_wall, &layout];
    let navigation = extract::navigation(bwem, &layout, shared);
    let size = bwem.size();

    let common = shared.then(|| {
        let mut out = String::new();
        writeln!(out, "answers bweb 1").unwrap();
        writeln!(out, "map {}", fixture.map).unwrap();
        bweb_common(&mut out, size, &navigation);
        out
    });

    let mut own = String::new();
    writeln!(own, "answers bweb 1").unwrap();
    writeln!(own, "map {}", fixture.map).unwrap();
    writeln!(own, "race {}", race.id()).unwrap();
    bweb_own(&mut own, bwem, &snapshots, &stations, &blocks, &walls);

    let path_tiles = extract::path_tiles(bwem);
    let first_mode = if shared { 0 } else { UNIT_MODE };
    let (common, own) = bweb_searches(common, own, &navigation, &path_tiles, first_mode);

    let mut plan = extract::plan(
        bwem,
        &stations,
        &walls,
        &blocks,
        snapshots.iter().map(|s| extract::snapshot(s)).collect(),
        race.id(),
        start.start_location,
    );
    plan.navigation = navigation;
    let analysis = extract::analysis(bwem, found);
    let terrain = Terrain::new(input, &[]);
    let start = property::bweb::Start {
        creep: start.creep.cells().to_vec(),
        depot: start.depot.cells().to_vec(),
    };
    let cx = property::bweb::Context {
        map: name,
        analysis: &analysis,
        terrain: &terrain,
        start: &start,
    };
    let case = Case {
        side: Side::Port,
        map: name.to_owned(),
        race: race_name.to_owned(),
        state: "analysis".to_owned(),
    };
    let mut violations = property::bweb::check(&plan, &cx);
    violations.extend(property::nav::check(&plan, &cx));
    violations.extend(property::production::check(&plan, &cx));
    (common, own, (case, violations))
}

/// What does not depend on the race: walkability, and from every point the
/// ground distances, the choke point paths and the closest choke tiles.
fn bweb_common(out: &mut String, size: TilePosition, navigation: &Navigation) {
    let width = size.x;
    for (ty, walkable) in WALK_TYPES.iter().zip(&navigation.walkable) {
        let name = format!("walkable {}", ty.id());
        grid_line(out, &name, size, |t| {
            i64::from(walkable[(t.y * width + t.x) as usize])
        });
    }

    let points = &navigation.points;
    let n = points.len();
    writeln!(out, "from {n} points: ground distance, choke point path").unwrap();
    for (i, &(x, y)) in points.iter().enumerate() {
        let distances = navigation.ground_distances[i * n..(i + 1) * n]
            .iter()
            .map(|d| d.map_or(-1, |d| d.to_bits() as i64));
        // An unreachable point has an empty path.
        let paths = navigation.choke_paths[i * n..(i + 1) * n]
            .iter()
            .flat_map(|path| {
                std::iter::once(path.len() as i64).chain(path.iter().map(|&cp| i64::from(cp)))
            });
        writeln!(
            out,
            "{x},{y}: {} {}",
            answer::hash(distances),
            answer::hash(paths)
        )
        .unwrap();
    }
    writeln!(out, "closest choke tiles").unwrap();
    for (cp, found) in &navigation.closest_choke_tiles {
        let found = found.iter().flat_map(|t| {
            let (x, y) = t.unwrap_or((32000, 32000));
            [i64::from(x), i64::from(y)]
        });
        writeln!(out, "{cp}: {}", answer::hash(found)).unwrap();
    }
}

/// The layout of the race: used and reserved tiles after the blocks and at
/// each stage before, placement, stations, blocks and walls.
fn bweb_own(
    out: &mut String,
    bwem: &Map,
    snapshots: &[&bweb::Layout; 4],
    stations: &Stations,
    blocks: &Blocks,
    walls: &Walls,
) {
    let size = bwem.size();
    let layout = snapshots[3];
    grid_line(out, "used", size, |t| {
        i64::from(layout.used_by(t, 1, 1).map_or(-1, UnitType::id) + 1)
    });
    grid_line(out, "reserved", size, |t| {
        i64::from(layout.is_reserved(t, 1, 1))
    });
    // The layout after `onStart`, the stations and the wall; the lines above
    // are after the blocks.
    for (stage, snapshot) in snapshots[..3].iter().enumerate() {
        grid_line(out, &format!("used at stage {stage}"), size, |t| {
            i64::from(snapshot.used_by(t, 1, 1).map_or(-1, UnitType::id) + 1)
        });
        grid_line(out, &format!("reserved at stage {stage}"), size, |t| {
            i64::from(snapshot.is_reserved(t, 1, 1))
        });
    }
    for ty in PLACE_TYPES {
        let name = format!("placeable {}", ty.id());
        grid_line(out, &name, size, |t| {
            i64::from(layout.can_place(bwem, ty, t))
        });
    }

    let location = |s: Option<&bweb::Station>| {
        s.map_or("-".to_owned(), |s| tile(bwem.base(s.base()).location()))
    };
    writeln!(out, "starting_main {}", location(stations.own_main())).unwrap();
    writeln!(out, "starting_natural {}", location(stations.own_natural())).unwrap();
    let slot = |t: Option<TilePosition>| t.map_or("-".to_owned(), tile);
    writeln!(out, "stations {}", stations.iter().len()).unwrap();
    for s in stations {
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
    for b in blocks {
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

/// The searches of `navigation`, run between `path_tiles` in the modes from
/// `first_mode` on, a line per unit type, mode and source: those over the
/// terrain to `common`, those over the buildings to `own`.
fn bweb_searches(
    common: Option<String>,
    mut own: String,
    navigation: &Navigation,
    path_tiles: &[TilePosition],
    first_mode: i32,
) -> (Option<String>, String) {
    let mut runs = navigation.searches.iter();
    let mut searches: BTreeMap<(i32, i32, usize), Vec<i64>> = BTreeMap::new();
    for ty in PATH_TYPES {
        for i in 0..path_tiles.len() {
            for _ in path_tiles {
                for mode in first_mode..=UNIT_MODE {
                    let path = runs.next().expect("a search of each kind");
                    let row = searches.entry((ty.id(), mode, i)).or_default();
                    row.push(i64::from(path.reachable));
                    row.push(path.distance.to_bits() as i64);
                    row.push(path.tiles.len() as i64);
                    row.extend(
                        path.tiles
                            .iter()
                            .flat_map(|&(x, y)| [i64::from(x), i64::from(y)]),
                    );
                }
            }
        }
    }
    let mut common = common.unwrap_or_default();
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
    ((first_mode == 0).then_some(common), own)
}

/// How the map `map` differs from the numbers of BWEM-community, if it is one
/// of its maps.
fn community(map: &str, bwem: &Map, found: bool) -> Option<(String, Vec<String>)> {
    let mut mismatches = Vec::new();
    if map == SHOWDOWN {
        let got = (
            bwem.size(),
            bwem.max_altitude(),
            bwem.starting_locations().len(),
            bwem.base_count(),
            found,
        );
        let want = (TilePosition::new(64, 192), 426, 2, 8, true);
        if got != want {
            mismatches.push(format!(
                "BWEM-community: got {got:?}, want {want:?} (size, max altitude, starts, bases, all found)"
            ));
        }
    } else {
        let &(_, (width, height), max_altitude) = SSCAIT
            .iter()
            .find(|(name, _, _)| map.strip_prefix("scai/") == Some(name))?;
        let got = (
            bwem.size(),
            bwem.walk_size(),
            bwem.center(),
            bwem.max_altitude(),
        );
        let want = (
            TilePosition::new(width, height),
            WalkPosition::new(width * 4, height * 4),
            Position::new(width * 16, height * 16),
            max_altitude,
        );
        if got != want {
            mismatches.push(format!("BWEM-community: got {got:?}, want {want:?}"));
        }
    }
    mismatches.extend(
        invariants(bwem)
            .into_iter()
            .map(|v| format!("invariant: {v}")),
    );
    Some((map.to_owned(), mismatches))
}

/// What any analysis must satisfy.
fn invariants(map: &Map) -> Vec<String> {
    let mut violations = Vec::new();

    for area in map.areas() {
        let id = area.id().get();
        // Each area is one 4-connected zone of its mini tiles.
        let mut seen = BTreeSet::from([area.top()]);
        let mut stack = vec![area.top()];
        while let Some(w) = stack.pop() {
            for (dx, dy) in [(0, -1), (-1, 0), (1, 0), (0, 1)] {
                let next = w + WalkPosition::new(dx, dy);
                if map.mini_tile(next).is_some_and(|m| m.area_id() == id) && seen.insert(next) {
                    stack.push(next);
                }
            }
        }
        let total = map.mini_tiles().filter(|m| m.area_id() == id).count();
        if seen.len() != total || total != area.size_in_mini_tiles() as usize {
            violations.push(format!(
                "area {id}: {} connected, {total} in all, {} counted",
                seen.len(),
                area.size_in_mini_tiles()
            ));
        }

        // Each choke point lies between this area and the other one.
        for cp in area.choke_points() {
            let (a, b) = cp.areas();
            if a != area && b != area {
                violations.push(format!(
                    "area {id}: choke point {:?} of other areas",
                    cp.id()
                ));
            }
            for node in [Node::End1, Node::Middle, Node::End2] {
                for side in [a, b] {
                    let w = cp.node_in_area(node, side.id());
                    if map.mini_tile(w).map(|m| m.area_id()) != Some(side.id().get()) {
                        violations.push(format!("{:?} {node:?}: {w:?} not in {side:?}", cp.id()));
                    }
                }
            }
        }

        // Areas of one group reach each other.
        for other in map.areas() {
            let reachable = map
                .choke_point_path(Position::from(area.top()), Position::from(other.top()))
                .is_some();
            if reachable != (area.group() == other.group()) {
                violations.push(format!("{area:?} {other:?}: group and reachability differ"));
            }
        }
    }

    // A resource belongs to one base at most.
    let mut assigned = BTreeSet::new();
    for area in map.areas() {
        for base in area.bases() {
            let resources = base
                .minerals()
                .map(|m| m.id())
                .chain(base.geysers().map(|g| g.id()));
            for r in resources {
                if !assigned.insert(r) {
                    violations.push(format!("{r:?} assigned twice"));
                }
            }
        }
    }
    violations
}

/// The reference the altitudes are checked against: small grids with known
/// distances, then random ones against every source tried.
#[test]
fn the_distance_transform_is_exact() {
    let one = edt::distances(5, 3, |x, y| (x, y) == (1, 1), false);
    assert_eq!(one[0], Some(2f64.sqrt()));
    assert_eq!(one[5 + 1], Some(0.0));
    assert_eq!(one[2 * 5 + 4], Some(10f64.sqrt()));
    assert!(
        edt::distances(3, 3, |_, _| false, false)
            .iter()
            .all(Option::is_none)
    );
    // The ring alone: one past the nearest edge.
    let (w, h) = (7, 4);
    let ring = edt::distances(w, h, |_, _| false, true);
    for y in 0..h {
        for x in 0..w {
            let edge = (x + 1).min(w - x).min(y + 1).min(h - y);
            assert_eq!(ring[(y * w + x) as usize], Some(f64::from(edge)), "{x},{y}");
        }
    }

    let mut state = 0x2545_f491_4f6c_dd1du64;
    let mut next = |n: u64| {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state % n) as i32
    };
    for _ in 0..500 {
        let (w, h) = (1 + next(14), 1 + next(14));
        let density = 1 + next(30);
        let ring = next(2) == 1;
        let sources: Vec<bool> = (0..w * h).map(|_| next(100) < density).collect();
        let got = edt::distances(w, h, |x, y| sources[(y * w + x) as usize], ring);
        for y in 0..h {
            for x in 0..w {
                let mut best: Option<i32> = None;
                for sy in -1..=h {
                    for sx in -1..=w {
                        let outside = sx < 0 || sy < 0 || sx >= w || sy >= h;
                        let is_source = if outside {
                            ring
                        } else {
                            sources[(sy * w + sx) as usize]
                        };
                        if is_source {
                            let squared = (sx - x).pow(2) + (sy - y).pow(2);
                            best = Some(best.map_or(squared, |b| b.min(squared)));
                        }
                    }
                }
                let expected = best.map(|s| f64::from(s).sqrt());
                assert_eq!(
                    got[(y * w + x) as usize],
                    expected,
                    "{w}x{h} ring {ring}: {x},{y}"
                );
            }
        }
    }
}
