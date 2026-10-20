//! Properties of results: what a correct result of BWEM, BWEB or FAP satisfies,
//! however it was computed. They are to let the SDK leave the algorithms of the
//! originals safely, not to tie it to them.
//!
//! The checks are pure functions over plain records. The tests of `bwapi-sdk`
//! fill the records from the port, `openbw-host` from the C++ originals; the
//! checks know neither. Where a check rests on a rule of the original rather
//! than on correctness, its name starts with `bwem_`, `bweb_` or `fap_` and its
//! comment says where the rule is.
//!
//! A violation is a bug, unless `tests/data/known.txt` lists it case by case:
//! side, map, race, state, property, subject and the bug of
//! `bw_eb_em_bugs.md` that explains it. A new violation fails, and so does a
//! listed one that no longer occurs: a fixed bug or a check that stopped
//! working, either is looked into. Only once a bug is in the registry and its
//! side and property in [`EXPLAINED`] does `UPDATE_KNOWN=<property>` list the
//! violations of that property; nothing lists them all at once. Once the port
//! is fixed, its lines go and those of C++ stay.
//!
//! Where the original does not recompute a structure after an event, the
//! structure is checked only in the state it was computed in.

#![allow(dead_code)]

pub mod bweb;
pub mod bwem;
pub mod edt;
pub mod nav;
pub mod production;

use std::collections::BTreeSet;
use std::path::Path;

/// What computed a result.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Side {
    Port,
    Cpp,
}

/// Where a result was computed.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Case {
    pub side: Side,
    /// The name under `maps/`, without its extension.
    pub map: String,
    /// `terran`, `zerg`, `protoss`, or empty where the race does not matter.
    pub race: String,
    /// `analysis`, or `without <unit id>` after a neutral was destroyed.
    pub state: String,
}

/// The file of the known violations, in the data of the tests of `bwapi-sdk`
/// at `data`.
pub fn known_file(data: &Path) -> std::path::PathBuf {
    data.join("known.txt")
}

/// A result that breaks a property.
#[derive(Clone, Debug)]
pub struct Violation {
    pub property: String,
    /// What breaks it, such as `area 3` or `base 12,40`: with the property,
    /// what a line of the known violations matches.
    pub subject: String,
    pub detail: String,
}

/// The properties whose violations a bug explains, by side: the only ones
/// `known.txt` may list.
pub const EXPLAINED: &[(Side, &str, u32)] = &[
    (Side::Port, "path_found_when_one_exists", 5),
    (Side::Cpp, "path_found_when_one_exists", 5),
    (Side::Port, "bweb_production_reachable", 13),
    (Side::Cpp, "bweb_production_reachable", 13),
    (Side::Port, "area_top", 28),
    (Side::Cpp, "area_top", 28),
    (Side::Port, "bwem_area_connected4", 29),
    (Side::Cpp, "bwem_area_connected4", 29),
    (Side::Port, "base_blocking_mineral_once", 30),
    (Side::Cpp, "base_blocking_mineral_once", 30),
    (Side::Port, "choke_point_path_iff_reachable", 32),
    (Side::Cpp, "choke_point_path_iff_reachable", 32),
    (Side::Port, "choke_point_reachable_in_group", 32),
    (Side::Cpp, "choke_point_reachable_in_group", 32),
    (Side::Port, "choke_point_shortest", 32),
    (Side::Cpp, "choke_point_shortest", 32),
    (Side::Port, "altitude_equals_exact_distance", 33),
    (Side::Cpp, "altitude_equals_exact_distance", 33),
    (Side::Port, "slot_on_map", 34),
    (Side::Cpp, "slot_on_map", 34),
    (Side::Port, "slot_buildable", 34),
    (Side::Cpp, "slot_buildable", 34),
    (Side::Port, "base_station_at_most_once", 36),
    (Side::Cpp, "base_station_at_most_once", 36),
    (Side::Port, "neutral_tiles_stay_used", 37),
    (Side::Cpp, "neutral_tiles_stay_used", 37),
    (Side::Port, "unit_walkable_off_neutrals", 37),
    (Side::Cpp, "unit_walkable_off_neutrals", 37),
    (Side::Port, "station_choke_borders_area", 39),
    (Side::Cpp, "station_choke_borders_area", 39),
    (Side::Port, "wall_defense_off_station_depot", 40),
    (Side::Cpp, "wall_defense_off_station_depot", 40),
    (Side::Port, "path_shortest", 41),
    (Side::Cpp, "path_shortest", 41),
    (Side::Port, "medic_does_not_heal_self", 42),
    (Side::Cpp, "medic_does_not_heal_self", 42),
    (Side::Port, "hit_within_damage", 43),
    (Side::Cpp, "hit_within_damage", 43),
];

impl Side {
    fn name(self) -> &'static str {
        match self {
            Side::Port => "port",
            Side::Cpp => "cpp",
        }
    }

    fn from_name(name: &str) -> Option<Side> {
        match name {
            "port" => Some(Side::Port),
            "cpp" => Some(Side::Cpp),
            _ => None,
        }
    }
}

/// The lines a child process prints of its checked cases, for its parent to
/// [`judge`] them all at once: `PROPERTY_CASE` and `PROPERTY_VIOLATION` with
/// tab separated fields.
pub fn report(checked: &[(Case, Vec<Violation>)]) -> Vec<String> {
    let mut lines = Vec::new();
    for (case, violations) in checked {
        let c = [case.side.name(), &case.map, &case.race, &case.state].join("\t");
        lines.push(format!("PROPERTY_CASE\t{c}"));
        for v in violations {
            lines.push(format!(
                "PROPERTY_VIOLATION\t{c}\t{}\t{}\t{}",
                v.property, v.subject, v.detail
            ));
        }
    }
    lines
}

/// The checked cases of the lines of [`report`] among `output`.
pub fn read_report(output: &str) -> Vec<(Case, Vec<Violation>)> {
    let mut checked: Vec<(Case, Vec<Violation>)> = Vec::new();
    for line in output.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        let case = |f: &[&str]| Case {
            side: Side::from_name(f[0]).expect("a side"),
            map: f[1].to_owned(),
            race: f[2].to_owned(),
            state: f[3].to_owned(),
        };
        match f[0] {
            "PROPERTY_CASE" if f.len() == 5 => checked.push((case(&f[1..]), Vec::new())),
            "PROPERTY_VIOLATION" if f.len() == 8 => {
                let case = case(&f[1..]);
                let v = Violation {
                    property: f[5].to_owned(),
                    subject: f[6].to_owned(),
                    detail: f[7].to_owned(),
                };
                match checked.iter_mut().rev().find(|(c, _)| *c == case) {
                    Some((_, violations)) => violations.push(v),
                    None => checked.push((case, vec![v])),
                }
            }
            _ => {}
        }
    }
    checked
}

/// A line of `known.txt`, tab separated: side, map, race, state, property,
/// subject, bug.
fn line(case: &Case, property: &str, subject: &str, bug: u32) -> String {
    [
        case.side.name(),
        &case.map,
        &case.race,
        &case.state,
        property,
        subject,
        &bug.to_string(),
    ]
    .join("\t")
}

/// The case of a line: its first four fields.
fn case_key(line: &str) -> String {
    line.splitn(5, '\t').take(4).collect::<Vec<_>>().join("\t")
}

/// The prefix of a diagnostic: cases to collect and show, which fail nothing
/// until what they mean is decided.
pub const DIAGNOSTIC: &str = "diag_";

/// The diagnostics among the checked cases, a line each.
pub fn diagnostics(checked: &[(Case, Vec<Violation>)]) -> Vec<String> {
    let mut lines = Vec::new();
    for (case, violations) in checked {
        for v in violations
            .iter()
            .filter(|v| v.property.starts_with(DIAGNOSTIC))
        {
            lines.push(format!(
                "{:?} {} {} {}: {} of {}: {}",
                case.side, case.map, case.race, case.state, v.property, v.subject, v.detail
            ));
        }
    }
    lines
}

/// The property of a line of `known.txt`: its fifth field.
fn property_of(line: &str) -> &str {
    line.split('\t').nth(4).unwrap_or("")
}

/// What fails among the violations of the checked cases, against the known
/// violations in `known` (`tests/data/known.txt`): violations it does not
/// list, and its lines of checked cases that no longer occur.
///
/// `UPDATE_KNOWN=<property>[,<property>]` writes anew the lines of the named
/// properties in the checked cases, from their violations now, once the bug
/// that explains them is in [`EXPLAINED`]; lines of other properties stay as
/// they are and still fail if they no longer occur.
pub fn judge(checked: &[(Case, Vec<Violation>)], known: &Path) -> Vec<String> {
    let update: BTreeSet<String> = std::env::var("UPDATE_KNOWN")
        .unwrap_or_default()
        .split(',')
        .filter(|p| !p.is_empty())
        .map(str::to_owned)
        .collect();
    let stored = std::fs::read_to_string(known).unwrap_or_default();
    let cases: BTreeSet<String> = checked
        .iter()
        .map(|(case, _)| case_key(&line(case, "", "", 0)))
        .collect();
    let listed: BTreeSet<&str> = stored
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
        .collect();
    let rewritten = |l: &str| cases.contains(&case_key(l)) && update.contains(property_of(l));

    let mut failures = Vec::new();
    for property in &update {
        let sides: BTreeSet<Side> = checked.iter().map(|(case, _)| case.side).collect();
        for side in sides {
            if !EXPLAINED
                .iter()
                .any(|&(s, p, _)| s == side && p == property)
            {
                failures.push(format!(
                    "UPDATE_KNOWN names {property}, which no bug of EXPLAINED explains for {side:?}"
                ));
            }
        }
    }
    let mut occurring = BTreeSet::new();
    for (case, violations) in checked {
        for v in violations
            .iter()
            .filter(|v| !v.property.starts_with(DIAGNOSTIC))
        {
            let bug = EXPLAINED
                .iter()
                .find(|&&(side, property, _)| side == case.side && property == v.property)
                .map(|&(_, _, bug)| bug);
            let known_line = bug.map(|bug| line(case, &v.property, &v.subject, bug));
            match known_line {
                Some(l) if listed.contains(l.as_str()) || update.contains(&v.property) => {
                    occurring.insert(l);
                }
                _ => failures.push(format!(
                    "{:?} {} {} {}: {} of {}: {}",
                    case.side, case.map, case.race, case.state, v.property, v.subject, v.detail
                )),
            }
        }
    }
    for l in listed
        .iter()
        .filter(|l| cases.contains(&case_key(l)) && !rewritten(l))
    {
        if !occurring.contains(*l) {
            failures.push(format!("no longer occurs: {}", l.replace('\t', " | ")));
        }
    }

    if !update.is_empty() && failures.is_empty() {
        let mut lines: BTreeSet<String> = listed
            .iter()
            .filter(|l| !rewritten(l))
            .map(|l| l.to_string())
            .collect();
        lines.extend(
            occurring
                .into_iter()
                .filter(|l| update.contains(property_of(l))),
        );
        let mut text = String::from(
            "# Known violations of properties: side, map, race, state, property, subject, bug of\n\
             # bw_eb_em_bugs.md, tab separated. See tests/property: UPDATE_KNOWN=<property>.\n",
        );
        for l in lines {
            text.push_str(&l);
            text.push('\n');
        }
        if let Err(e) = std::fs::write(known, text) {
            failures.push(format!("{}: {e}", known.display()));
        }
    }
    failures
}

/// The cells breaking one property: reported once, as the first of them with
/// their count.
pub struct Offenders {
    property: &'static str,
    /// What the cells are, such as `mini tiles`.
    unit: &'static str,
    /// Position, the value found and the value expected.
    all: Vec<(i32, i32, i32, i32)>,
    pub first: Option<String>,
}

impl Offenders {
    pub fn new(property: &'static str, unit: &'static str) -> Self {
        Offenders {
            property,
            unit,
            all: Vec::new(),
            first: None,
        }
    }

    pub fn add(
        &mut self,
        at: (i32, i32),
        actual: i32,
        expected: i32,
        detail: impl FnOnce() -> String,
    ) {
        self.all.push((at.0, at.1, actual, expected));
        if self.first.is_none() {
            self.first = Some(format!("{},{}: {}", at.0, at.1, detail()));
        }
    }

    /// An offender that is not a cell, such as a search: `key` names it and
    /// its values for the hash, `detail` tells it.
    pub fn add_keyed(&mut self, key: (i32, i32, i32, i32), detail: impl FnOnce() -> String) {
        self.all.push(key);
        if self.first.is_none() {
            self.first = Some(detail());
        }
    }

    /// One violation: the subject names the count and a hash of the sorted
    /// list, so that a known violation matches only the same set.
    pub fn report(mut self, fail: &mut impl FnMut(&'static str, String, String)) {
        let Some(first) = self.first else { return };
        self.all.sort_unstable();
        let values = self
            .all
            .iter()
            .flat_map(|&(x, y, a, e)| [x, y, a, e].map(i64::from));
        fail(
            self.property,
            format!("{} {}, hash {}", self.all.len(), self.unit, hash(values)),
            format!("the first at {first}"),
        );
    }
}

/// FNV-1a over the values: names a set of offenders in one subject.
pub fn hash(values: impl IntoIterator<Item = i64>) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for v in values {
        for byte in v.to_le_bytes() {
            h ^= u64::from(byte);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
    }
    format!("{h:016x}")
}

/// Marks in `seen`, a grid `width` wide row by row, the cells reached
/// 4-connected from `start` through cells where `inside` holds and not yet
/// seen; their count.
pub fn fill4(
    seen: &mut [bool],
    width: i32,
    height: i32,
    start: (i32, i32),
    inside: impl Fn(i32, i32) -> bool,
) -> i32 {
    let mut stack = vec![start];
    let mut count = 0;
    seen[(start.1 * width + start.0) as usize] = true;
    while let Some((x, y)) = stack.pop() {
        count += 1;
        for (dx, dy) in [(0, -1), (-1, 0), (1, 0), (0, 1)] {
            let (nx, ny) = (x + dx, y + dy);
            if (0..width).contains(&nx) && (0..height).contains(&ny) {
                let i = (ny * width + nx) as usize;
                if !seen[i] && inside(nx, ny) {
                    seen[i] = true;
                    stack.push((nx, ny));
                }
            }
        }
    }
    count
}
