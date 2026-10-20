//! BWEM on the fixtures of real maps. Each map is analysed once as Terran, and
//! what the analysis gives is checked three ways:
//!
//! - against the answers next to each fixture: `bwem.txt` for the analysis,
//!   then after each blocking neutral and a few minerals are destroyed one by
//!   one;
//! - for the properties of `tests/property`, with the known violations;
//! - against the numbers of the tests of BWEM-community, on their maps.
//!
//! The analysis in full, but for the mini tiles: their altitudes and areas are
//! hashes, and so are the distances and paths between choke points, a line per
//! choke point. After a destruction every part is a hash, with the length of
//! each list.

mod answer;
mod extract;
mod fixture;
mod property;
mod scenario;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::PathBuf;

use bwapi::{Position, Race, TilePosition, WalkPosition};
use bwapi_sdk::bwem::{Map, MapInput, Node};
use bwapi_sdk::grid::Grid;
use fixture::Fixture;
use property::bwem::{Terrain, When};
use property::edt;
use property::{Case, Side, Violation};

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
struct Checked {
    answers: Vec<(PathBuf, String)>,
    cases: Cases,
    community: Option<(String, Vec<String>)>,
}

#[test]
fn real_maps_give_their_answers_and_have_their_properties() {
    let fixtures = answer::fixtures();
    let results = answer::parallel(&fixtures, |path| {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let fixture = Fixture::parse(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        check(&fixture, path)
    });

    let mut problems: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut cases = Vec::new();
    let mut community = BTreeSet::new();
    for (path, result) in fixtures.iter().zip(results) {
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

/// The map of `fixture`, at `path`, as Terran: the analysis of BWEM, the map
/// after the destructions and the numbers of BWEM-community.
fn check(fixture: &Fixture, path: &std::path::Path) -> Result<Checked, String> {
    let name = fixture
        .map
        .rsplit_once('.')
        .map_or(&*fixture.map, |(n, _)| n);
    let start = fixture
        .start(Race::TERRAN)
        .ok_or_else(|| "no start as terran".to_owned())?;
    let input = fixture.map_input(start);
    let mut map = Map::initialize(&input).map_err(|e| format!("refused: {e}"))?;
    let found = map.find_bases_for_starting_locations();

    let community = community(&fixture.map, &map, found);
    let (text, cases) = bwem(fixture, &input, &mut map, found, name)?;
    Ok(Checked {
        answers: vec![(path.with_file_name("bwem.txt"), text)],
        cases,
        community,
    })
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
