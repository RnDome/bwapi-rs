//! BWEM-community in C++ (`BWEM_DIR`), run on the game in progress: what the
//! Rust port of BWEM is compared with.

use std::ffi::c_void;

/// What C++ BWEM computed for the map of the game in progress.
#[derive(Clone, Debug, Default)]
pub struct Reference {
    pub max_altitude: i32,
    pub walk_size: (i32, i32),
    /// `FindBasesForStartingLocations()`.
    pub bases_found: bool,
    /// Every mini tile, row by row.
    pub altitudes: Vec<i32>,
    pub mini_tile_areas: Vec<i32>,
    /// Every tile, row by row.
    pub tile_areas: Vec<i32>,
    pub areas: Vec<AreaRecord>,
    /// In the order of `Graph::ChokePoints()`.
    pub choke_points: Vec<ChokePointRecord>,
    /// `DistanceFrom`, index by index.
    pub distances: Vec<i32>,
    /// Area by area.
    pub bases: Vec<BaseRecord>,
    /// `GetPathTo`, index by index: the indices of the choke points.
    pub paths: Vec<Vec<i32>>,
    /// The map after each neutral [`analyse`] destroyed: its unit id, and the
    /// map once it and those before it are gone.
    pub after: Vec<(i32, Reference)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AreaRecord {
    pub id: i32,
    pub top: (i32, i32),
    pub mini_tiles: i32,
    pub group_id: i32,
    pub top_left: (i32, i32),
    pub bottom_right: (i32, i32),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChokePointRecord {
    pub index: i32,
    pub areas: (i32, i32),
    pub end1: (i32, i32),
    pub middle: (i32, i32),
    pub end2: (i32, i32),
    pub pseudo: bool,
    pub blocked: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BaseRecord {
    pub area: i32,
    pub location: (i32, i32),
    pub starting: bool,
    /// Unit ids.
    pub minerals: Vec<i32>,
    pub geysers: Vec<i32>,
    pub blocking_minerals: Vec<i32>,
}

type Sink = extern "C" fn(*mut c_void, i32, *const i32, usize);

unsafe extern "C" {
    fn openbw_host_bwem_reference(
        ctx: *mut c_void,
        sink: Sink,
        destroyed: *const i32,
        destroyed_len: usize,
    );
}

extern "C" fn collect(ctx: *mut c_void, kind: i32, values: *const i32, len: usize) {
    // SAFETY: `ctx` is the `Vec` of `analyse`; C++ passes `len` readable values.
    let records = unsafe { &mut *ctx.cast::<Vec<(i32, Vec<i32>)>>() };
    let values = if len == 0 {
        Vec::new()
    } else {
        unsafe { std::slice::from_raw_parts(values, len) }.to_vec()
    };
    records.push((kind, values));
}

/// Runs C++ BWEM on the game in progress, then destroys the neutrals of the
/// unit ids `destroyed` one by one: `OnMineralDestroyed` for mineral fields,
/// `OnStaticBuildingDestroyed` for the others. Call it from a callback of a
/// bot, once per process: BWEM keeps a `static` counter across analyses.
/// `None` if C++ threw.
pub fn analyse(destroyed: &[i32]) -> Option<Reference> {
    let mut records: Vec<(i32, Vec<i32>)> = Vec::new();
    // SAFETY: `collect` reads `records` only during the call; `destroyed`
    // outlives it.
    unsafe {
        openbw_host_bwem_reference(
            (&raw mut records).cast(),
            collect,
            destroyed.as_ptr(),
            destroyed.len(),
        )
    };

    let mut states = vec![(0, Reference::default())];
    for (kind, v) in records {
        if kind == 9 {
            states.push((v.first().copied().unwrap_or(-1), Reference::default()));
            continue;
        }
        let r = &mut states.last_mut().expect("the first state").1;
        let at = |i: usize| v.get(i).copied().unwrap_or(-1);
        let xy = |i: usize| (at(i), at(i + 1));
        match kind {
            0 => {
                r.max_altitude = at(0);
                r.walk_size = xy(1);
                r.bases_found = at(3) == 1;
            }
            1 => r.altitudes = v,
            2 => r.mini_tile_areas = v,
            3 => r.tile_areas = v,
            4 => r.areas.push(AreaRecord {
                id: at(0),
                top: xy(1),
                mini_tiles: at(3),
                group_id: at(4),
                top_left: xy(5),
                bottom_right: xy(7),
            }),
            5 => r.choke_points.push(ChokePointRecord {
                index: at(0),
                areas: xy(1),
                end1: xy(3),
                middle: xy(5),
                end2: xy(7),
                pseudo: at(9) == 1,
                blocked: at(10) == 1,
            }),
            6 => r.distances = v,
            7 => {
                let (minerals, geysers, blocking) = (
                    at(4).max(0) as usize,
                    at(5).max(0) as usize,
                    at(6).max(0) as usize,
                );
                let mut ids = v.get(7..).unwrap_or(&[]).iter().copied();
                r.bases.push(BaseRecord {
                    area: at(0),
                    location: xy(1),
                    starting: at(3) == 1,
                    minerals: ids.by_ref().take(minerals).collect(),
                    geysers: ids.by_ref().take(geysers).collect(),
                    blocking_minerals: ids.take(blocking).collect(),
                });
            }
            8 => {
                let mut rest = v.get(1..).unwrap_or(&[]);
                while let Some((&len, tail)) = rest.split_first() {
                    let len = (len.max(0) as usize).min(tail.len());
                    r.paths.push(tail[..len].to_vec());
                    rest = &tail[len..];
                }
            }
            _ => return None,
        }
    }
    let mut states = states.into_iter();
    let (_, mut r) = states.next().expect("the first state");
    r.after = states.collect();
    Some(r)
}
