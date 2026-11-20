//! The fixtures of the tests of `bwapi-sdk`: what BWAPI reports at the start of
//! a game on each map, as each race, against the files in
//! `crates/sdk/tests/data`. With `UPDATE_FIXTURES=1` the files are written
//! instead. Each map runs in a process of its own, so that maps run in
//! parallel. A fixture must give the input `adapter` reads from the game.

#[path = "../../sdk/tests/fixture/mod.rs"]
mod fixture;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::Instant;

use bwapi::{Bot, Context, Race, TilePosition, WalkPosition};
use bwapi_sdk::adapter::{map_input, start_state};
use bwapi_sdk::bweb::StartState;
use bwapi_sdk::bwem::NeutralInput;
use bwapi_sdk::grid::Grid;
use fixture::{Fixture, Start};

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

const RACES: [Race; 3] = [Race::TERRAN, Race::ZERG, Race::PROTOSS];

const MAP_VAR: &str = "FIXTURE_MAP";
const OUT_VAR: &str = "FIXTURE_OUT";

/// What the last game reported, and how its fixture differs from what
/// `adapter` reads.
static RECORDED: Mutex<Option<(Fixture, Vec<String>)>> = Mutex::new(None);

struct Recorder;

impl<'game> Bot<'game> for Recorder {
    fn on_start(cx: Context<'_, 'game>) -> Self {
        let game = cx.game();
        let me = game.self_player().expect("the bot plays");
        let (width, height) = (game.map_width(), game.map_height());
        let tiles = |f: &dyn Fn(TilePosition) -> bool| Grid::from_fn(width, height, f);
        let fixture = Fixture {
            map: String::new(),
            hash: game.map_hash().to_string(),
            size: (width, height),
            start_locations: game.start_locations(),
            walkable: Grid::from_fn(width * 4, height * 4, |w: WalkPosition| game.is_walkable(w)),
            buildable: tiles(&|t| game.is_buildable(t)),
            ground_height: Grid::from_fn(width, height, |t: TilePosition| game.ground_height(t)),
            starts: vec![Start {
                race: me.race(),
                start_location: me.start_location(),
                enemy: game.enemy().map(|e| e.race()),
                neutrals: game
                    .static_neutral_units()
                    .iter(cx)
                    .map(|u| NeutralInput {
                        unit_id: u.id(),
                        unit_type: u.unit_type(),
                        initial_type: u.initial_type(),
                        initial_tile_position: u.initial_tile_position(),
                        initial_position: u.initial_position(),
                        initial_resources: u.initial_resources(),
                    })
                    .collect(),
                creep: tiles(&|t| game.has_creep(t)),
                depot: tiles(&|t| game.can_build_here(t, me.race().resource_depot())),
            }],
        };

        // The fixture gives what the adapter reads, to the tiles around the
        // map.
        let mut differences = Vec::new();
        let ours = &fixture.starts[0];
        if fixture.map_input(ours) != map_input(cx) {
            differences.push("the input differs from adapter::map_input".to_owned());
        }
        let theirs = start_state(cx);
        let depot = me.race().resource_depot();
        for y in -1..=height {
            for x in -1..=width {
                let t = TilePosition::new(x, y);
                if ours.has_creep(t) != theirs.has_creep(t) {
                    differences.push(format!("creep at {t:?}"));
                }
                if ours.can_build_here(t, depot) != theirs.can_build_here(t, depot) {
                    differences.push(format!("canBuildHere at {t:?}"));
                }
            }
        }
        *RECORDED.lock().unwrap() = Some((fixture, differences));
        Recorder
    }
}

bwapi::bot!(Recorder);

/// The directory of the data of the tests of `bwapi-sdk`.
fn data() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../sdk/tests/data")
}

/// One map as every race, in a child process of `fixtures_are_the_game`:
/// writes the fixture to `FIXTURE_OUT`, prints what is wrong.
#[test]
#[ignore = "run by fixtures_are_the_game, one process per map"]
fn one_map() {
    let map = PathBuf::from(std::env::var_os(MAP_VAR).expect("FIXTURE_MAP is the path of the map"));
    let out = std::env::var_os(OUT_VAR).expect("FIXTURE_OUT is the path of the fixture");
    let mut merged: Option<Fixture> = None;
    let mut problems = Vec::new();
    for race in RACES {
        openbw_host::play_as(&map, 1, race.id(), gameInit, newAIModule).unwrap();
        let (fixture, differences) = RECORDED
            .lock()
            .unwrap()
            .take()
            .expect("the bot never started");
        problems.extend(differences.into_iter().map(|d| format!("as {race:?}: {d}")));
        if fixture.starts[0].race != race {
            problems.push(format!(
                "the bot played {:?}, not {race:?}",
                fixture.starts[0].race
            ));
        }
        match &mut merged {
            None => merged = Some(fixture),
            Some(m) => {
                let start = fixture.starts[0].clone();
                let map_of = |f: &Fixture| Fixture {
                    starts: Vec::new(),
                    ..f.clone()
                };
                if map_of(m) != map_of(&fixture) {
                    problems.push(format!("as {race:?}: the map differs from the first race"));
                }
                m.starts.push(start);
            }
        }
    }
    let mut fixture = merged.expect("a race");
    fixture.map = openbw_host::map_name(&map);
    let text = fixture.to_text();
    match Fixture::parse(&text) {
        Ok(read) if read == fixture => {}
        Ok(_) => problems.push("the text reads back as another fixture".to_owned()),
        Err(e) => problems.push(format!("the text does not read back: {e}")),
    }
    std::fs::write(&out, text).unwrap();
    for p in &problems {
        println!("FIXTURE {p}");
    }
}

#[test]
fn fixtures_are_the_game() {
    let exe = std::env::current_exe().unwrap();
    let update = std::env::var_os("UPDATE_FIXTURES").is_some();
    let tmp = Path::new(env!("CARGO_TARGET_TMPDIR")).join("fixtures");
    std::fs::create_dir_all(&tmp).unwrap();
    let maps = openbw_host::parity_maps(MAPS);
    let start = Instant::now();
    let outputs = openbw_host::parallel(&maps, |map| {
        let name = openbw_host::map_name(map);
        let out = tmp.join(format!("{}.txt", name.replace('/', "_")));
        let output = Command::new(&exe)
            .args(["one_map", "--exact", "--ignored", "--nocapture"])
            .env(MAP_VAR, map)
            .env(OUT_VAR, &out)
            .output()
            .unwrap();
        (name, out, output)
    });
    let seconds = start.elapsed().as_secs_f64();

    let mut problems: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let (mut written, mut bytes) = (0, 0);
    for (name, out, output) in outputs {
        let mut lines: Vec<String> = String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter_map(|l| l.strip_prefix("FIXTURE "))
            .map(str::to_owned)
            .collect();
        let Ok(text) = std::fs::read_to_string(&out) else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            lines.push(format!(
                "no fixture: {}",
                stderr.lines().last().unwrap_or("")
            ));
            problems.insert(name, lines);
            continue;
        };
        bytes += text.len();
        let file = fixture::path(&data(), &name);
        let stored = std::fs::read_to_string(&file).ok();
        if stored.as_deref() != Some(text.as_str()) {
            if update {
                std::fs::create_dir_all(file.parent().unwrap()).unwrap();
                std::fs::write(&file, &text).unwrap();
                written += 1;
            } else if let Some(stored) = stored {
                let line = stored
                    .lines()
                    .zip(text.lines())
                    .position(|(a, b)| a != b)
                    .map_or(stored.lines().count().min(text.lines().count()), |i| i)
                    + 1;
                lines.push(format!(
                    "{} differs from the game from line {line}",
                    file.display()
                ));
            } else {
                lines.push(format!(
                    "no {}: UPDATE_FIXTURES=1 writes it",
                    file.display()
                ));
            }
        }
        if !lines.is_empty() {
            problems.insert(name, lines);
        }
    }
    println!(
        "{} maps, {bytes} bytes of fixtures, {written} written: {seconds:.1} s",
        maps.len()
    );
    assert!(problems.is_empty(), "{problems:#?}");
}
