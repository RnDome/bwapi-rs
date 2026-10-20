//! The properties of `tests/property/bwem` on the analyses of the port: on the
//! fixtures of real maps as Terran, then after each neutral the answers destroy.
//! `openbw-host`'s `bwem_parity` checks C++ BWEM-community the same way.

mod answer;
mod extract;
mod fixture;
mod property;
mod scenario;

use bwapi::Race;
use bwapi_sdk::bwem::Map;
use extract::analysis;
use fixture::Fixture;
use property::bwem::{self, Terrain, When};
use property::edt;
use property::{Case, Side, Violation};

/// The violations of each state of the map of `fixture`.
fn check(fixture: &Fixture) -> Result<Vec<(Case, Vec<Violation>)>, String> {
    let start = fixture
        .start(Race::TERRAN)
        .ok_or("the fixture has no start as Terran")?;
    let input = fixture.map_input(start);
    let mut map = Map::initialize(&input).map_err(|e| format!("refused: {e}"))?;
    let found = map.find_bases_for_starting_locations();
    let name = fixture
        .map
        .rsplit_once('.')
        .map_or(&*fixture.map, |(n, _)| n);
    let case = |state: String| Case {
        side: Side::Port,
        map: name.to_owned(),
        race: String::new(),
        state,
    };
    let mut checked = vec![(
        case("analysis".into()),
        bwem::check(
            &analysis(&map, found),
            &Terrain::new(&input, &[]),
            When::Analysis,
        ),
    )];
    let mut destroyed = Vec::new();
    for id in scenario::neutrals_to_destroy(&map) {
        if map.remove_neutral(id).is_none() {
            return Err(format!("no neutral of unit {id}"));
        }
        destroyed.push(id);
        checked.push((
            case(format!("without {id}")),
            bwem::check(
                &analysis(&map, found),
                &Terrain::new(&input, &destroyed),
                When::AfterDestruction,
            ),
        ));
    }
    Ok(checked)
}

#[test]
fn bwem_results_have_their_properties() {
    let fixtures = answer::fixtures();
    let results = answer::parallel(&fixtures, |path| {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let fixture = Fixture::parse(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        check(&fixture).map_err(|e| format!("{}: {e}", path.display()))
    });
    let mut checked = Vec::new();
    let mut failures = Vec::new();
    for r in results {
        match r {
            Ok(cases) => checked.extend(cases),
            Err(e) => failures.push(e),
        }
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
