//! The properties of `tests/property/bweb` on what BWEB of the port makes of
//! the fixtures of real maps as each race: `onStart`, the stations, the wall of
//! the race at the starting natural, the blocks, as `openbw-host`'s
//! `bweb_parity` runs them on C++ and checks the same.

mod answer;
mod extract;
mod fixture;
mod property;

use bwapi::Race;
use bwapi_sdk::bweb::{self, Blocks, Stations, WallSpec, Walls};
use bwapi_sdk::bwem::Map;
use fixture::Fixture;
use property::bweb::{self as checks, Context, Start};
use property::bwem::Terrain;
use property::{Case, Side, Violation};

const RACES: [(Race, &str); 3] = [
    (Race::TERRAN, "terran"),
    (Race::ZERG, "zerg"),
    (Race::PROTOSS, "protoss"),
];

/// The violations of the plan of BWEB for the map of `fixture` as `race`.
fn check(fixture: &Fixture, race: Race, name: &str) -> Result<(Case, Vec<Violation>), String> {
    let start = fixture
        .start(race)
        .ok_or_else(|| format!("no start as {name}"))?;
    let input = fixture.map_input(start);
    let mut bwem = Map::initialize(&input).map_err(|e| format!("refused: {e}"))?;
    let found = bwem.find_bases_for_starting_locations();

    let mut layout = bweb::Layout::new(&bwem, &input);
    let mut snapshots = vec![extract::snapshot(&layout)];
    let stations = Stations::analyze(&bwem, &mut layout, start.start_location, race);
    snapshots.push(extract::snapshot(&layout));
    let mut walls = Walls::new(&bwem);
    let enemy = start.enemy.unwrap_or(Race::NONE);
    let _ = WallSpec::natural(race, enemy, &bwem, &stations)
        .and_then(|spec| walls.build(&bwem, &mut layout, &stations, start, &spec));
    snapshots.push(extract::snapshot(&layout));
    let blocks = Blocks::find(&bwem, &mut layout, &stations, race, start);
    snapshots.push(extract::snapshot(&layout));

    let mut plan = extract::plan(
        &bwem,
        &stations,
        &walls,
        &blocks,
        snapshots,
        race.id(),
        start.start_location,
    );
    // What does not depend on the race is searched as Terran only, as in
    // `bweb_parity`.
    plan.navigation = extract::navigation(&bwem, &layout, race == Race::TERRAN);
    let analysis = extract::analysis(&bwem, found);
    let terrain = Terrain::new(&input, &[]);
    let start = Start {
        creep: start.creep.cells().to_vec(),
        depot: start.depot.cells().to_vec(),
    };
    let map = fixture
        .map
        .rsplit_once('.')
        .map_or(&*fixture.map, |(n, _)| n);
    let cx = Context {
        map,
        analysis: &analysis,
        terrain: &terrain,
        start: &start,
    };
    let case = Case {
        side: Side::Port,
        map: map.to_owned(),
        race: name.to_owned(),
        state: "analysis".to_owned(),
    };
    let mut violations = checks::check(&plan, &cx);
    violations.extend(property::nav::check(&plan, &cx));
    violations.extend(property::production::check(&plan, &cx));
    Ok((case, violations))
}

#[test]
fn bweb_results_have_their_properties() {
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
        let (race, name) = RACES[r];
        check(fixture, race, name).map_err(|e| format!("{}: {e}", fixtures[m].display()))
    });
    let mut checked = Vec::new();
    let mut failures = Vec::new();
    for r in results {
        match r {
            Ok(case) => checked.push(case),
            Err(e) => failures.push(e),
        }
    }
    for line in property::diagnostics(&checked) {
        println!("{line}");
    }
    failures.extend(property::judge(
        &checked,
        &property::known_file(&answer::data()),
    ));
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
