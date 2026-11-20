//! The Rust port of BWEM against BWEM-community in C++, on the same game. Each
//! map runs in a process of its own: C++ BWEM keeps a `static` counter across
//! analyses, so a second analysis in one process is not what a bot gets.
//!
//! Every state of C++ is also checked for the properties of the tests of
//! `bwapi-sdk` (`crates/sdk/tests/property`).

#![cfg(bwem_reference)]

#[path = "../../sdk/tests/property/mod.rs"]
mod property;

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;
use std::sync::Mutex;

use bwapi::{Bot, Context};
use bwapi_sdk::adapter::map_input;
use bwapi_sdk::bwem::{Map, MapInput, NeutralKind};
use openbw_host::bwem_reference::{self, AreaRecord, BaseRecord, ChokePointRecord, Reference};
use property::bwem::{self as properties, Analysis, Terrain, When};
use property::{Case, Side, Violation};

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
];

const MAP_VAR: &str = "BWEM_PARITY_MAP";

static REPORT: Mutex<Option<Vec<String>>> = Mutex::new(None);

/// The properties of C++, as `property::report` prints them for the parent to
/// judge.
static PROPERTIES: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// The neutrals C++ destroyed after the analysis: blocking ones, and all.
static DESTROYED: Mutex<(usize, usize)> = Mutex::new((0, 0));

struct Comparer;

impl<'game> Bot<'game> for Comparer {
    fn on_start(cx: Context<'_, 'game>) -> Self {
        let input = map_input(cx);
        let report = match Map::initialize(&input) {
            Err(e) => vec![format!("the port refused the map: {e}")],
            Ok(mut map) => {
                let found = map.find_bases_for_starting_locations();
                let (destroyed, blocking) = neutrals_to_destroy(&map);
                match bwem_reference::analyse(&destroyed) {
                    None => vec!["C++ BWEM threw".to_owned()],
                    Some(reference) => {
                        *DESTROYED.lock().unwrap() =
                            (blocking.min(reference.after.len()), reference.after.len());
                        let mut report = compare(&map, found, &reference);
                        if reference.after.len() != destroyed.len() {
                            report.push(format!(
                                "C++ destroyed {} of {} neutrals",
                                reference.after.len(),
                                destroyed.len()
                            ));
                        }
                        for (id, after) in &reference.after {
                            if map.remove_neutral(*id).is_none() {
                                report.push(format!("the port has no neutral of unit {id}"));
                            }
                            let differences = compare(&map, found, after);
                            report.extend(
                                differences
                                    .into_iter()
                                    .map(|d| format!("without unit {id}: {d}")),
                            );
                        }
                        *PROPERTIES.lock().unwrap() =
                            property::report(&properties_of_cpp(&input, &reference));
                        report
                    }
                }
            }
        };
        *REPORT.lock().unwrap() = Some(report);
        Comparer
    }
}

/// The analysis C++ gave, as the properties take it.
fn analysis(r: &Reference) -> Analysis {
    Analysis {
        size: (r.walk_size.0 / 4, r.walk_size.1 / 4),
        max_altitude: r.max_altitude,
        altitudes: r.altitudes.clone(),
        mini_tile_areas: r.mini_tile_areas.clone(),
        tile_areas: r.tile_areas.clone(),
        areas: r
            .areas
            .iter()
            .map(|a| properties::Area {
                id: a.id,
                top: a.top,
                mini_tiles: a.mini_tiles,
                group: a.group_id,
                top_left: a.top_left,
                bottom_right: a.bottom_right,
            })
            .collect(),
        choke_points: r
            .choke_points
            .iter()
            .map(|cp| properties::ChokePoint {
                index: cp.index,
                areas: cp.areas,
                nodes: [cp.end1, cp.middle, cp.end2],
                pseudo: cp.pseudo,
                blocked: cp.blocked,
                geometry: Vec::new(),
            })
            .collect(),
        bases: r
            .bases
            .iter()
            .map(|b| properties::Base {
                area: b.area,
                location: b.location,
                starting: b.starting,
                minerals: b.minerals.clone(),
                geysers: b.geysers.clone(),
                blocking_minerals: b.blocking_minerals.clone(),
            })
            .collect(),
        bases_found: r.bases_found,
        distances: r.distances.clone(),
        paths: r.paths.clone(),
    }
}

/// What C++ breaks of the properties, in every state.
fn properties_of_cpp(input: &MapInput, r: &Reference) -> Vec<(Case, Vec<Violation>)> {
    let path = std::env::var_os(MAP_VAR).expect("BWEM_PARITY_MAP is the path of the map");
    let name = openbw_host::map_name(Path::new(&path));
    let map = name.rsplit_once('.').map_or(name.as_str(), |(n, _)| n);
    let case = |state: String| Case {
        side: Side::Cpp,
        map: map.to_owned(),
        race: String::new(),
        state,
    };
    let mut checked = vec![(
        case("analysis".into()),
        properties::check(&analysis(r), &Terrain::new(input, &[]), When::Analysis),
    )];
    let mut destroyed = Vec::new();
    for (id, after) in &r.after {
        destroyed.push(*id);
        checked.push((
            case(format!("without {id}")),
            properties::check(
                &analysis(after),
                &Terrain::new(input, &destroyed),
                When::AfterDestruction,
            ),
        ));
    }
    checked
}

/// The minerals destroyed after the analysis, besides the blocking neutrals.
const OTHER_MINERALS: usize = 8;

/// The unit ids of the neutrals destroyed one by one after the analysis: every
/// blocking one, which opens choke points and changes areas of tiles, then a
/// few other minerals, which leave their bases. And how many are blocking.
fn neutrals_to_destroy(map: &Map) -> (Vec<i32>, usize) {
    let of_kind = |kind| map.neutrals().filter(move |n| n.kind() == kind);
    let mut ids: Vec<i32> = of_kind(NeutralKind::StaticBuilding)
        .chain(of_kind(NeutralKind::Mineral))
        .filter(|n| n.is_blocking())
        .map(|n| n.unit_id())
        .collect();
    let blocking = ids.len();
    ids.extend(
        of_kind(NeutralKind::Mineral)
            .filter(|n| !n.is_blocking())
            .take(OTHER_MINERALS)
            .map(|n| n.unit_id()),
    );
    (ids, blocking)
}

bwapi::bot!(Comparer);

/// One map, in a child process of `the_port_matches_bwem_community`.
#[test]
#[ignore = "run by the_port_matches_bwem_community, one process per map"]
fn one_map() {
    let map = std::env::var_os(MAP_VAR).expect("BWEM_PARITY_MAP is the path of the map");
    openbw_host::play(Path::new(&map), 1, gameInit, newAIModule).unwrap();
    let report = REPORT
        .lock()
        .unwrap()
        .take()
        .expect("the bot never started");
    for line in &report {
        println!("PARITY {line}");
    }
    for line in PROPERTIES.lock().unwrap().iter() {
        println!("{line}");
    }
    let (blocking, all) = *DESTROYED.lock().unwrap();
    println!("PARITY_DESTROYED {blocking} {all}");
    assert!(report.is_empty());
}

#[test]
fn the_port_matches_bwem_community() {
    let exe = std::env::current_exe().unwrap();
    let maps = openbw_host::parity_maps(MAPS);
    let destroyed = Mutex::new((0, 0));
    let checked = Mutex::new(Vec::new());
    let reports = openbw_host::parallel(&maps, |map| {
        let out = Command::new(&exe)
            .args(["one_map", "--exact", "--ignored", "--nocapture"])
            .env(MAP_VAR, map)
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&out.stdout);
        checked
            .lock()
            .unwrap()
            .extend(property::read_report(&stdout));
        if let Some(counts) = stdout
            .lines()
            .find_map(|l| l.strip_prefix("PARITY_DESTROYED "))
        {
            let mut counts = counts.split(' ').map(|c| c.parse::<usize>().unwrap_or(0));
            let mut total = destroyed.lock().unwrap();
            total.0 += counts.next().unwrap_or(0);
            total.1 += counts.next().unwrap_or(0);
        }
        let mut lines: Vec<String> = stdout
            .lines()
            .filter_map(|l| l.strip_prefix("PARITY "))
            .map(str::to_owned)
            .collect();
        if !out.status.success() && lines.is_empty() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            lines.push(format!(
                "the child failed: {}",
                stderr.lines().last().unwrap_or("")
            ));
        }
        lines
    });
    let mut differences: BTreeMap<String, Vec<String>> = maps
        .iter()
        .zip(reports)
        .filter(|(_, lines)| !lines.is_empty())
        .map(|(map, lines)| (openbw_host::map_name(map), lines))
        .collect();
    let data = Path::new(env!("CARGO_MANIFEST_DIR")).join("../sdk/tests/data");
    let failures = property::judge(&checked.into_inner().unwrap(), &property::known_file(&data));
    if !failures.is_empty() {
        differences.insert("properties of C++".to_owned(), failures);
    }
    assert!(differences.is_empty(), "{differences:#?}");
    // A comparison after no destruction passes.
    let (blocking, all) = destroyed.into_inner().unwrap();
    println!("destroyed {all} neutrals, {blocking} of them blocking");
    assert!(
        blocking > 0 && all > blocking,
        "destroyed {all}, {blocking} blocking"
    );
}

/// The differences between the port and C++, at most a few of each kind.
fn compare(map: &Map, found: bool, r: &Reference) -> Vec<String> {
    let mut out = Vec::new();
    let header = (
        i32::from(map.max_altitude()),
        (map.walk_size().x, map.walk_size().y),
        found,
    );
    if header != (r.max_altitude, r.walk_size, r.bases_found) {
        out.push(format!(
            "max altitude, walk size, bases found: port {header:?}, C++ {:?}",
            (r.max_altitude, r.walk_size, r.bases_found)
        ));
    }

    let width = map.walk_size().x;
    let cells = |name: &str, ours: Vec<i32>, theirs: &[i32], width: i32, out: &mut Vec<String>| {
        let diffs: Vec<String> = ours
            .iter()
            .zip(theirs)
            .enumerate()
            .filter(|(_, (a, b))| a != b)
            .map(|(i, (a, b))| {
                format!(
                    "({}, {}): port {a}, C++ {b}",
                    i as i32 % width,
                    i as i32 / width
                )
            })
            .collect();
        if !diffs.is_empty() || ours.len() != theirs.len() {
            out.push(format!(
                "{name}: {} of {} differ, e.g. {:?}",
                diffs.len(),
                theirs.len(),
                &diffs[..diffs.len().min(3)]
            ));
        }
    };
    cells(
        "mini tile altitudes",
        map.mini_tiles().map(|m| i32::from(m.altitude())).collect(),
        &r.altitudes,
        width,
        &mut out,
    );
    cells(
        "mini tile areas",
        map.mini_tiles().map(|m| i32::from(m.area_id())).collect(),
        &r.mini_tile_areas,
        width,
        &mut out,
    );
    cells(
        "tile areas",
        map.tiles().map(|t| i32::from(t.area_id())).collect(),
        &r.tile_areas,
        map.size().x,
        &mut out,
    );

    let areas: Vec<AreaRecord> = map
        .areas()
        .map(|a| AreaRecord {
            id: i32::from(a.id().get()),
            top: (a.top().x, a.top().y),
            mini_tiles: a.size_in_mini_tiles(),
            group_id: i32::from(a.group()),
            top_left: (a.bounds().0.x, a.bounds().0.y),
            bottom_right: (a.bounds().1.x, a.bounds().1.y),
        })
        .collect();
    list("areas", &areas, &r.areas, &mut out);

    let mut cps: Vec<_> = map.areas().flat_map(|a| a.choke_points()).collect();
    cps.sort_by_key(|cp| cp.id());
    cps.dedup_by_key(|cp| cp.id());
    let ours: Vec<ChokePointRecord> = cps
        .iter()
        .map(|cp| {
            let (a, b) = cp.areas();
            let xy = |n| {
                let w = cp.node(n);
                (w.x, w.y)
            };
            ChokePointRecord {
                index: cp.id().index() as i32,
                areas: (i32::from(a.id().get()), i32::from(b.id().get())),
                end1: xy(bwapi_sdk::bwem::Node::End1),
                middle: xy(bwapi_sdk::bwem::Node::Middle),
                end2: xy(bwapi_sdk::bwem::Node::End2),
                pseudo: cp.is_pseudo(),
                blocked: cp.is_blocked(),
            }
        })
        .collect();
    let mut theirs = r.choke_points.clone();
    theirs.sort_by_key(|cp| cp.index);
    list("choke points", &ours, &theirs, &mut out);

    let distances: Vec<i32> = cps
        .iter()
        .flat_map(|a| cps.iter().map(move |b| a.distance_to(b.id()).unwrap_or(-1)))
        .collect();
    cells(
        "choke point distances",
        distances,
        &r.distances,
        cps.len().max(1) as i32,
        &mut out,
    );

    let paths: Vec<Vec<i32>> = cps
        .iter()
        .flat_map(|a| {
            // An unreachable choke point has an empty path.
            cps.iter().map(move |b| {
                a.path_to(b.id())
                    .map(|p| p.iter().map(|cp| cp.id().index() as i32).collect())
                    .unwrap_or_default()
            })
        })
        .collect();
    list("choke point paths", &paths, &r.paths, &mut out);

    let bases: Vec<BaseRecord> = map
        .areas()
        .flat_map(|a| a.bases())
        .map(|b| BaseRecord {
            area: i32::from(b.area().id().get()),
            location: (b.location().x, b.location().y),
            starting: b.is_starting(),
            minerals: b.minerals().map(|m| m.unit_id()).collect(),
            geysers: b.geysers().map(|g| g.unit_id()).collect(),
            blocking_minerals: b.blocking_minerals().map(|m| m.unit_id()).collect(),
        })
        .collect();
    list("bases", &bases, &r.bases, &mut out);
    out
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
