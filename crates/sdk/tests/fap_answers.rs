//! FAP on the random fights of `tests/fight` against its answers
//! (`tests/data/fap.txt`): a line per fight with the units of each side at the
//! start, after the steps and after the long run, and hashes of every state.
//! `openbw-host`'s `fap_parity` compares the same fights with FAP in C++.

mod answer;
mod fight;

use std::fmt::Write as _;

use fight::{FRAMES_PER_STEP, LONG_RUN, STEPS, State};

/// The fights, each from its own seed: those of `fap_parity`.
const FIGHTS: u64 = 20_000;

fn sides(state: &State) -> String {
    format!("{}:{}", state[0].len(), state[1].len())
}

fn hash(states: &[State]) -> String {
    answer::hash(states.iter().flat_map(|s| s.iter()).flat_map(|side| {
        std::iter::once(side.len() as i64).chain(side.iter().flatten().map(|&v| i64::from(v)))
    }))
}

#[test]
fn fap_gives_its_answers() {
    let seeds: Vec<u64> = (0..FIGHTS).collect();
    let lines = answer::parallel(&seeds, |&seed| {
        let (fighters, tank_splash) = fight::fight(seed);
        let steps = fight::simulate(&fighters, tank_splash, STEPS, FRAMES_PER_STEP);
        let long = fight::simulate(&fighters, tank_splash, 1, LONG_RUN);
        format!(
            "{seed}{} {} {} {} {} {}",
            if tank_splash { " splash" } else { "" },
            sides(&steps[0]),
            sides(steps.last().expect("a state")),
            sides(long.last().expect("a state")),
            hash(&steps),
            hash(&long)
        )
    });
    let mut text = String::new();
    writeln!(text, "answers fap 1").unwrap();
    writeln!(
        text,
        "seed, splash, units of each side at the start, after {STEPS} steps of \
         {FRAMES_PER_STEP} frames and after {LONG_RUN} frames; hashes of the states"
    )
    .unwrap();
    for line in lines {
        writeln!(text, "{line}").unwrap();
    }
    if let Err(e) = answer::check(&answer::data().join("fap.txt"), &text) {
        panic!("{e}");
    }
}
