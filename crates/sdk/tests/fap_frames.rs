//! FAP frame by frame on fights of `fap_properties` against its answers
//! (`tests/data/fap_frames.txt`): a line per fight with hashes of what
//! `openbw-host`'s `fap_parity` compares with FAP in C++ — every state of a
//! run frame by frame, the run at once and its idle check — and nothing else.
//! A regression of the port on a sample of the fights; `fap_parity` compares
//! all of them with C++.

mod answer;
mod fight;
mod property;

use std::fmt::Write as _;

use bwapi::UnitType;
use fight::Fighter;
use property::fap::{self, FRAMES, Fight, Kind, LONG_FIGHTS, LONG_RUN, Made, Message};

/// The first fights of `fap_properties`: the long ones and as many short.
const FIGHTS: u64 = 2_000;

/// The port on a fight of `fighters`, frame by frame for `limit` frames, as
/// `fap_properties` observes it.
fn observe(
    name: String,
    id: i32,
    fighters: &[Fighter],
    splash: bool,
    made: Made,
    limit: i32,
) -> Fight {
    let kinds = fighters
        .iter()
        .map(|f| Kind::of(f.player, f.unit_type))
        .collect();
    fap::observe(
        name,
        id,
        kinds,
        UnitType::TERRAN_MARINE.max_hit_points() << 8,
        made,
        limit,
        |mode, frames| fight::run(fighters, splash, mode, frames),
    )
}

/// The random fights, then the controlled ones, as `fap_properties` makes
/// them.
fn fights() -> Vec<Fight> {
    let seeds: Vec<u64> = (0..FIGHTS).collect();
    let mut fights = answer::parallel(&seeds, |&seed| {
        let (fighters, splash) = fight::fight(seed);
        let limit = if seed < LONG_FIGHTS { LONG_RUN } else { FRAMES };
        observe(
            format!("fight {seed}"),
            seed as i32,
            &fighters,
            splash,
            Made::Random,
            limit,
        )
    });
    for (i, (name, fighters, splash, no_attacks)) in fight::controlled().into_iter().enumerate() {
        let made = if no_attacks {
            Made::ControlledWithoutAttacks
        } else {
            Made::Controlled
        };
        fights.push(observe(
            name.to_owned(),
            1_000_000 + i as i32,
            &fighters,
            splash,
            made,
            FRAMES,
        ));
    }
    fights
}

/// The values of a message, as `fap::parity` compares them: the reason and
/// frames of the outcome, -1 for none, then per side the count of units and
/// each unit's fields and tag. The counts make the bounds unambiguous.
fn values(message: &Message, out: &mut Vec<i64>) {
    let (reason, frames) = message.outcome.unwrap_or((-1, -1));
    out.extend([i64::from(reason), i64::from(frames)]);
    for side in &message.units {
        out.push(side.len() as i64);
        for unit in side {
            out.extend(unit.fields.iter().map(|&v| i64::from(v)));
            out.push(i64::from(unit.tag));
        }
    }
}

/// The count of the messages, then each of them.
fn hash<'a>(messages: impl ExactSizeIterator<Item = &'a Message>) -> String {
    let mut all = vec![messages.len() as i64];
    for m in messages {
        values(m, &mut all);
    }
    answer::hash(all)
}

/// The line of a fight.
fn line(f: &Fight) -> String {
    let (reason, at) = f.whole.outcome.unwrap_or((-1, -1));
    format!(
        "{} frames {} {} whole {reason} {at} {} idle {}",
        f.id,
        f.frames.len(),
        hash(f.frames.iter()),
        hash(std::iter::once(&f.whole)),
        f.idle_check
            .as_ref()
            .map_or("-".to_owned(), |m| hash(std::iter::once(m)))
    )
}

#[test]
fn fap_gives_its_frame_answers() {
    let mut text = String::new();
    writeln!(text, "answers fap frames 1").unwrap();
    writeln!(
        text,
        "id, frames observed and the hash of every state; reason and frames of \
         simulate(limit) at once and its hash; the hash of its idle check, - for none"
    )
    .unwrap();
    for f in fights() {
        writeln!(text, "{}", line(&f)).unwrap();
    }
    if let Err(e) = answer::check(&answer::data().join("fap_frames.txt"), &text) {
        panic!("{e}");
    }
}

/// A change of one field of one unit on one frame changes the line.
#[test]
fn the_line_of_a_fight_sees_every_frame() {
    let (fighters, splash) = fight::fight(0);
    let mut f = observe(
        "fight 0".to_owned(),
        0,
        &fighters,
        splash,
        Made::Random,
        FRAMES,
    );
    let before = line(&f);
    let last = f.frames.len() - 1;
    assert!(last > 1, "a fight of {} frames", f.frames.len());
    for frame in [0, last / 2, last] {
        let unit = f.frames[frame]
            .units
            .iter_mut()
            .flatten()
            .next()
            .expect("a unit on the frame");
        unit.fields[fap::HEALTH] += 1;
        let changed = line(&f);
        let unit = f.frames[frame].units.iter_mut().flatten().next().unwrap();
        unit.fields[fap::HEALTH] -= 1;
        assert_ne!(changed, before, "frame {frame}");
    }
    assert_eq!(line(&f), before);
}
