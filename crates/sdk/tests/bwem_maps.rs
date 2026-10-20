//! BWEM on the fixtures of real maps: the numbers of the tests of
//! BWEM-community and the invariants of an analysis.

mod answer;
mod fixture;

use std::collections::BTreeSet;

use bwapi::{Position, Race, TilePosition, WalkPosition};
use bwapi_sdk::bwem::{Map, Node};
use fixture::Fixture;

fn analyse(name: &str) -> (Map, bool) {
    let path = fixture::path(&answer::data(), name);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{name}: {e}"));
    let fixture = Fixture::parse(&text).unwrap_or_else(|e| panic!("{name}: {e}"));
    let start = fixture
        .start(Race::TERRAN)
        .unwrap_or_else(|| panic!("{name}: no start as Terran"));
    let mut map =
        Map::initialize(&fixture.map_input(start)).unwrap_or_else(|e| panic!("{name}: {e}"));
    let found = map.find_bases_for_starting_locations();
    (map, found)
}

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

#[test]
fn sscait_maps_match_bwem_community() {
    let mut mismatches = Vec::new();
    for (name, (width, height), max_altitude) in SSCAIT {
        let (map, _) = analyse(&format!("scai/{name}"));
        let got = (
            map.size(),
            map.walk_size(),
            map.center(),
            map.max_altitude(),
        );
        let want = (
            TilePosition::new(width, height),
            WalkPosition::new(width * 4, height * 4),
            Position::new(width * 16, height * 16),
            max_altitude,
        );
        if got != want {
            mismatches.push(format!("{name}: got {got:?}, want {want:?}"));
        }
        let violations = invariants(&map);
        if !violations.is_empty() {
            mismatches.push(format!("{name}: {violations:?}"));
        }
    }
    assert!(mismatches.is_empty(), "{mismatches:#?}");
}

#[test]
fn showdown_matches_bwem_community() {
    let (map, found) = analyse("scai/(2)Showdown.scx");
    assert_eq!(map.size(), TilePosition::new(64, 192));
    assert_eq!(map.max_altitude(), 426);
    assert_eq!(map.starting_locations().len(), 2);
    assert_eq!(map.base_count(), 8);
    assert!(found, "a start location got no base");
    assert_eq!(invariants(&map), Vec::<String>::new());
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
