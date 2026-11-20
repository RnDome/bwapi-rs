//! Games of the OpenBW fork of BWAPI inside a test process: a bot exported with
//! [`bwapi::bot!`] plays a real map without `BWAPILauncher`.
//!
//! Needs StarCraft's MPQ files, which cannot be distributed, so this crate is not
//! in `default-members` and does not run in public CI. Environment:
//!
//! - `OPENBW_BWAPI_DIR` (build): a built checkout of `github.com/openbw/bwapi`;
//!   `OPENBW_BWAPI_LIB_DIR` overrides its `build/lib`;
//! - `OPENBW_MPQ_PATH` (run): the directory with `StarDat.mpq`, `BrooDat.mpq` and
//!   `Patch_rt.mpq`;
//! - `PARITY_MAPS` (run, optional): a directory of maps, or `all` for the maps
//!   of the repository, `maps`; the comparisons with C++ then run on every map
//!   under it ([`parity_maps`]);
//! - `BWEM_DIR` (build, optional): a checkout of BWEM-community; with it,
//!   `bwem_reference` runs the C++ original for comparison;
//! - `BWEB_DIR` (build, optional, with `BWEM_DIR`): a checkout of BWEB v1.17;
//!   with it, `bweb_reference` runs the C++ original for comparison;
//! - `FAP_DIR` (build, optional): a checkout of FAP; with it, `fap_reference`
//!   runs the C++ original for comparison.
//!
//! Run the tests with `--release`: unoptimised, the comparisons with C++ take
//! several times longer.

#[cfg(bweb_reference)]
pub mod bweb_reference;
#[cfg(bwem_reference)]
pub mod bwem_reference;
#[cfg(fap_reference)]
pub mod fap_reference;

use std::ffi::{CString, c_char, c_void};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

unsafe extern "C" {
    fn openbw_host_play(
        map: *const c_char,
        game_init: extern "C" fn(*mut c_void),
        new_module: extern "C" fn() -> *mut c_void,
        frames: i32,
        race: i32,
    ) -> i32;
}

/// The BWAPI id of Terran, the race of [`play`].
pub const TERRAN: i32 = 1;

/// The game of the fork is a process global: one game at a time.
static GAME: Mutex<()> = Mutex::new(());

/// Plays `map` for at most `frames` frames with the bot whose entry points
/// `bwapi::bot!` exported: pass its `gameInit` and `newAIModule`. The bot gets
/// `on_start`, a callback per event and `on_end`, as under `BWAPILauncher`. It
/// plays Terran.
pub fn play(
    map: &Path,
    frames: i32,
    game_init: extern "C" fn(*mut c_void),
    new_module: extern "C" fn() -> *mut c_void,
) -> Result<(), String> {
    play_as(map, frames, TERRAN, game_init, new_module)
}

/// As [`play`], with the bot playing the race of the BWAPI id `race`
/// (`bwapi::Race::id()`).
pub fn play_as(
    map: &Path,
    frames: i32,
    race: i32,
    game_init: extern "C" fn(*mut c_void),
    new_module: extern "C" fn() -> *mut c_void,
) -> Result<(), String> {
    let mpq = std::env::var_os("OPENBW_MPQ_PATH")
        .map(PathBuf::from)
        .ok_or("OPENBW_MPQ_PATH is not set: point it at the directory with StarDat.mpq")?;
    if !mpq.join("StarDat.mpq").exists() {
        return Err(format!("no StarDat.mpq in {}", mpq.display()));
    }
    let path = map.to_str().ok_or("the map path is not UTF-8")?;
    let path = CString::new(path).map_err(|_| "the map path contains a zero byte")?;

    let _game = GAME.lock().unwrap_or_else(|e| e.into_inner());
    // SAFETY: `path` outlives the call; the entry points are those of `bwapi::bot!`.
    match unsafe { openbw_host_play(path.as_ptr(), game_init, new_module, frames, race) } {
        0 => Ok(()),
        _ => Err(format!("OpenBW could not play {}", map.display())),
    }
}

/// The maps of the repository: `scbw` of Blizzard, `scai` of bot tournaments,
/// `bwem` the test maps of BWEM-community.
pub fn test_maps() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../maps")
}

/// `name`, such as `scai/(2)Showdown.scx`, in [`test_maps`].
pub fn map(name: &str) -> PathBuf {
    test_maps().join(name)
}

/// `map` as [`map`] takes it, or its file name if it is not in [`test_maps`]:
/// for reports.
pub fn map_name(map: &Path) -> String {
    map.strip_prefix(test_maps())
        .unwrap_or_else(|_| Path::new(map.file_name().unwrap_or_default()))
        .to_string_lossy()
        .into_owned()
}

/// The maps a comparison with C++ runs on: with `PARITY_MAPS` set, every
/// `.scm` and `.scx` under that directory, or under [`test_maps`] for `all`;
/// otherwise `names`, resolved by [`map`].
pub fn parity_maps(names: &[&str]) -> Vec<PathBuf> {
    let Some(dir) = std::env::var_os("PARITY_MAPS") else {
        return names.iter().map(|name| map(name)).collect();
    };
    let dir = if dir == "all" {
        test_maps()
    } else {
        PathBuf::from(dir)
    };
    let mut maps = Vec::new();
    let mut dirs = vec![dir];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("PARITY_MAPS: cannot read {}: {e}", dir.display()))
        {
            let path = entry.expect("cannot read a directory entry").path();
            if path.is_dir() {
                dirs.push(path);
            } else if path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("scm") || e.eq_ignore_ascii_case("scx"))
            {
                maps.push(path);
            }
        }
    }
    maps.sort();
    maps
}

/// `job` of every item, run on as many threads as the machine has cores; the
/// results in the order of `items`. For comparisons that run a child process
/// per map.
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
