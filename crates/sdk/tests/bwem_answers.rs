//! BWEM on the fixtures of real maps against its answers (`bwem.txt` next to
//! each fixture): the analysis as Terran, then the map after each blocking
//! neutral and a few minerals are destroyed one by one. `openbw-host`'s
//! `bwem_parity` compares the same with C++ BWEM-community.
//!
//! The analysis in full, but for the mini tiles: their altitudes and areas are
//! hashes, and so are the distances and paths between choke points, a line per
//! choke point. After a destruction every part is a hash, with the length of
//! each list.

mod answer;
mod fixture;
mod scenario;

use std::collections::BTreeMap;
use std::fmt::Write as _;

use bwapi::{Race, TilePosition};
use bwapi_sdk::bwem::{Map, Node};
use bwapi_sdk::grid::Grid;
use fixture::Fixture;
use scenario::neutrals_to_destroy;

/// The map as it is now; in full, or with grids and matrices as hashes.
fn state(out: &mut String, map: &Map, found: bool, full: bool) {
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

/// The answer of a fixture, or why there is none.
fn answer(fixture: &Fixture) -> Result<String, String> {
    let start = fixture
        .start(Race::TERRAN)
        .ok_or("the fixture has no start as Terran")?;
    let mut map =
        Map::initialize(&fixture.map_input(start)).map_err(|e| format!("refused: {e}"))?;
    let found = map.find_bases_for_starting_locations();
    let mut out = String::new();
    writeln!(out, "answers bwem 1").unwrap();
    writeln!(out, "map {}", fixture.map).unwrap();
    state(&mut out, &map, found, true);
    for id in neutrals_to_destroy(&map) {
        if map.remove_neutral(id).is_none() {
            return Err(format!("no neutral of unit {id}"));
        }
        writeln!(out, "destroyed {id}").unwrap();
        state(&mut out, &map, found, false);
    }
    Ok(out)
}

#[test]
fn bwem_gives_its_answers() {
    let fixtures = answer::fixtures();
    let results = answer::parallel(&fixtures, |path| {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let fixture = Fixture::parse(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        let computed = answer(&fixture)?;
        answer::check(&path.with_file_name("bwem.txt"), &computed)
    });
    let problems: BTreeMap<String, Vec<String>> = fixtures
        .iter()
        .zip(results)
        .filter_map(|(path, r)| r.err().map(|e| (path.display().to_string(), vec![e])))
        .collect();
    answer::assert_none(problems);
}
