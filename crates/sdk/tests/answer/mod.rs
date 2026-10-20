//! The answers of the tests on fixtures: what the SDK computes, as text next to
//! the fixture of each map in `tests/data`. They were written by the port while
//! it matched the C++ originals in `openbw-host` on everything they hold, so
//! they are what C++ computes; `UPDATE_ANSWERS=1` writes them again, for a fix
//! that changes them. Bulky matrices are kept as hashes, a line per row.
//!
//! The 17 maps of the tests of BWEM-community by default; `ANSWERS=all` checks
//! every map with a fixture.

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

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
    // The Barracks of a game that its worker never reached.
    "scbw/(2)Road War.scm",
];

/// The data of the tests: fixtures and answers.
pub fn data() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data")
}

/// The fixtures to check: of [`MAPS`], or of every map with `ANSWERS=all`.
pub fn fixtures() -> Vec<PathBuf> {
    if std::env::var_os("ANSWERS").is_some_and(|a| a == "all") {
        let mut found = Vec::new();
        let mut dirs = vec![data()];
        while let Some(dir) = dirs.pop() {
            for entry in std::fs::read_dir(&dir).expect("tests/data") {
                let path = entry.expect("a directory entry").path();
                if path.is_dir() {
                    dirs.push(path);
                } else if path.file_name().is_some_and(|n| n == "input.txt") {
                    found.push(path);
                }
            }
        }
        found.sort();
        found
    } else {
        MAPS.iter()
            .map(|m| {
                data()
                    .join(Path::new(m).with_extension(""))
                    .join("input.txt")
            })
            .collect()
    }
}

/// `text` against `file`: `Ok` if they are the same, or once `UPDATE_ANSWERS`
/// has written `text` there; otherwise where they differ.
pub fn check(file: &Path, text: &str) -> Result<(), String> {
    let stored = std::fs::read_to_string(file).ok();
    if stored.as_deref() == Some(text) {
        return Ok(());
    }
    if std::env::var_os("UPDATE_ANSWERS").is_some() {
        std::fs::write(file, text).map_err(|e| format!("{}: {e}", file.display()))?;
        return Ok(());
    }
    let Some(stored) = stored else {
        return Err(format!("no {}: UPDATE_ANSWERS=1 writes it", file.display()));
    };
    let (mut a, mut b) = (stored.lines(), text.lines());
    for n in 1.. {
        match (a.next(), b.next()) {
            (Some(x), Some(y)) if x == y => {}
            (x, y) => {
                return Err(format!(
                    "{} line {n}:\n  expected {}\n  computed {}",
                    file.display(),
                    x.unwrap_or("<end>"),
                    y.unwrap_or("<end>")
                ));
            }
        }
    }
    unreachable!()
}

/// FNV-1a over the values: a line of a bulky answer.
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

/// `job` of every item on as many threads as the machine has; the results in
/// the order of `items`.
pub fn parallel<T: Sync, R: Send>(items: &[T], job: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let next = AtomicUsize::new(0);
    let results: Mutex<Vec<Option<R>>> = Mutex::new(items.iter().map(|_| None).collect());
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get());
    std::thread::scope(|scope| {
        for _ in 0..threads.min(items.len()) {
            scope.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(item) = items.get(i) else { break };
                    let result = job(item);
                    results.lock().unwrap()[i] = Some(result);
                }
            });
        }
    });
    results
        .into_inner()
        .unwrap()
        .into_iter()
        .map(|r| r.expect("every item is run"))
        .collect()
}

/// Fails with every problem, by map.
pub fn assert_none(problems: BTreeMap<String, Vec<String>>) {
    assert!(
        problems.is_empty(),
        "{} maps fail:\n{}",
        problems.len(),
        problems
            .iter()
            .map(|(map, lines)| format!("{map}:\n  {}", lines.join("\n  ")))
            .collect::<Vec<_>>()
            .join("\n")
    );
}
