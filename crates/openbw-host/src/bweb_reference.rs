//! BWEB v1.17 in C++ (`BWEB_DIR`) on BWEM-community (`BWEM_DIR`), run on the
//! game in progress: what the Rust port of BWEB is compared with.

use std::ffi::c_void;

/// What C++ BWEB computed for the map of the game in progress.
#[derive(Clone, Debug, Default)]
pub struct Reference {
    pub size: (i32, i32),
    /// `isWalkable` per walk type of [`analyse`], every tile row by row.
    pub walkable: Vec<Vec<bool>>,
    /// `isUsed`: the type id, every tile row by row.
    pub used: Vec<Option<i32>>,
    pub reserved: Vec<bool>,
    /// `isPlaceable` per placed type of [`analyse`], every tile row by row.
    pub placeable: Vec<Vec<bool>>,
    /// Per placed type, every tile: ground units or creep within two tiles of
    /// the building, which `canBuildHere` checks for a resource depot. Empty
    /// for the other types.
    pub occupied: Vec<Vec<bool>>,
    /// `getGroundDistance`, point by point; `f64::MAX` is `DBL_MAX`.
    pub ground_distances: Vec<f64>,
    /// BWEM's `GetPath`, point by point: the indices of the choke points.
    pub paths: Vec<Vec<i32>>,
    /// `getClosestChokeTile` per choke point index, point by point.
    pub closest_choke_tiles: Vec<(i32, Vec<(i32, i32)>)>,
    pub stations: Vec<StationRecord>,
    /// The locations of the bases of the starting main and natural.
    pub starting_main: Option<(i32, i32)>,
    pub starting_natural: Option<(i32, i32)>,
    /// The searches of `Path`, in the order of [`analyse`].
    pub searches: Vec<SearchRecord>,
    /// `Blocks::getBlocks`, after `findBlocks`.
    pub blocks: Vec<BlockRecord>,
    /// The wall of the race's preset at the starting natural, made before the
    /// blocks, if one was found.
    pub wall: Option<WallRecord>,
    /// The seconds C++ took for each of [`PHASES`].
    pub seconds: Vec<f64>,
    /// The used and reserved tiles after `onStart`, the stations, the wall
    /// and the blocks, in this order.
    pub snapshots: Vec<Snapshot>,
}

/// The tiles of the layout at a stage of the analysis, row by row.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    /// The type id of the building on the tile.
    pub used: Vec<Option<i32>>,
    pub reserved: Vec<bool>,
}

/// What [`Reference::seconds`] times, in its order.
pub const PHASES: [&str; 7] = [
    "BWEM",
    "BWEB",
    "walkable",
    "placeable",
    "occupied",
    "choke point queries",
    "searches",
];

/// What a `BWEB::Wall` holds.
#[derive(Clone, Debug, PartialEq)]
pub struct WallRecord {
    pub defense_angle: f64,
    /// The index of its choke point and the id of its area.
    pub choke: Option<i32>,
    pub area: Option<i32>,
    /// The top left tiles of the small, medium and large pieces and the
    /// openings, sorted.
    pub pieces: [Vec<(i32, i32)>; 4],
    /// The defenses by row, 0 for every row.
    pub defenses: Vec<(i32, Vec<(i32, i32)>)>,
}

/// What a `BWEB::Block` holds.
#[derive(Clone, Debug, PartialEq)]
pub struct BlockRecord {
    pub tile: (i32, i32),
    pub size: (i32, i32),
    pub center: (i32, i32),
    pub proxy: bool,
    pub defensive: bool,
    /// `BlockType`: None 0, Start, Production, Proxy, Defensive, Supply, Tech.
    pub kind: i32,
    /// The top left tiles of the small, medium, large and wide slots, sorted.
    pub slots: [Vec<(i32, i32)>; 4],
}

/// What a search of `BWEB::Path` gave.
#[derive(Clone, Debug, PartialEq)]
pub struct SearchRecord {
    pub reachable: bool,
    pub distance: f64,
    pub tiles: Vec<(i32, i32)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StationRecord {
    pub location: (i32, i32),
    pub main: bool,
    pub natural: bool,
    /// The index of the choke point.
    pub choke: Option<i32>,
    pub resource_centroid: (i32, i32),
    /// The race presets: the slot of the ComSat or of a Spire, of a Spawning
    /// Pool, the pocket defense (never set in v1.17), the defense angle, the
    /// defenses. `TilePositions::Invalid` for none.
    pub small: (i32, i32),
    pub medium: (i32, i32),
    pub pocket: (i32, i32),
    pub defense_angle: f64,
    pub defenses: Vec<(i32, i32)>,
}

type Sink = extern "C" fn(*mut c_void, i32, *const f64, usize);

unsafe extern "C" {
    fn openbw_host_bweb_reference(
        ctx: *mut c_void,
        sink: Sink,
        walk_types: *const i32,
        walk_len: usize,
        place_types: *const i32,
        place_len: usize,
        points: *const i32,
        point_len: usize,
        path_types: *const i32,
        path_type_len: usize,
        path_tiles: *const i32,
        tile_len: usize,
        first_mode: i32,
    );
}

extern "C" fn collect(ctx: *mut c_void, kind: i32, values: *const f64, len: usize) {
    // SAFETY: `ctx` is the `Vec` of `analyse`; C++ passes `len` readable values.
    let records = unsafe { &mut *ctx.cast::<Vec<(i32, Vec<f64>)>>() };
    let values = if len == 0 {
        Vec::new()
    } else {
        unsafe { std::slice::from_raw_parts(values, len) }.to_vec()
    };
    records.push((kind, values));
}

/// Runs C++ BWEM and BWEB on the game in progress: walkability for the unit
/// types `walk_types`, placement of `place_types`, ground distances and closest
/// choke tiles between and from `points` (pixels), then the searches of `Path`
/// for each of `path_types`, from each of `path_tiles` to each, in the modes
/// from `first_mode` on: 0 A* over `terrainWalkable`, 1 A* with the cost
/// `(x * 7 + y * 13) % 5 * 8` over it, 2 JPS over it, 3 JPS over
/// `unitWalkable`. Units and creep near a building are found for resource
/// depots only. Call it from a callback of a bot, once per process: both keep
/// globals. `None` if C++ threw.
pub fn analyse(
    walk_types: &[i32],
    place_types: &[i32],
    points: &[(i32, i32)],
    path_types: &[i32],
    path_tiles: &[(i32, i32)],
    first_mode: i32,
) -> Option<Reference> {
    let flat: Vec<i32> = points.iter().flat_map(|&(x, y)| [x, y]).collect();
    let tiles: Vec<i32> = path_tiles.iter().flat_map(|&(x, y)| [x, y]).collect();
    let mut records: Vec<(i32, Vec<f64>)> = Vec::new();
    // SAFETY: the slices outlive the call; `collect` reads `records` only during it.
    unsafe {
        openbw_host_bweb_reference(
            (&raw mut records).cast(),
            collect,
            walk_types.as_ptr(),
            walk_types.len(),
            place_types.as_ptr(),
            place_types.len(),
            flat.as_ptr(),
            flat.len(),
            path_types.as_ptr(),
            path_types.len(),
            tiles.as_ptr(),
            tiles.len(),
            first_mode,
        )
    };

    let mut r = Reference::default();
    for (kind, v) in records {
        let at = |i: usize| v.get(i).map_or(-1, |&x| x as i32);
        let xy = |i: usize| (at(i), at(i + 1));
        let flags = || v.iter().map(|&x| x != 0.0).collect::<Vec<bool>>();
        match kind {
            0 => r.size = xy(0),
            1 => r.walkable.push(flags()),
            2 => r.used = v.iter().map(|&x| (x >= 0.0).then_some(x as i32)).collect(),
            3 => r.reserved = flags(),
            4 => r.placeable.push(flags()),
            5 => r.occupied.push(flags()),
            6 => r.ground_distances = v,
            7 => r
                .closest_choke_tiles
                .push((at(0), (1..v.len()).step_by(2).map(xy).collect())),
            8 => r.stations.push(StationRecord {
                location: xy(0),
                main: at(2) == 1,
                natural: at(3) == 1,
                choke: (at(4) >= 0).then_some(at(4)),
                resource_centroid: xy(5),
                small: xy(7),
                medium: xy(9),
                pocket: xy(11),
                defense_angle: v.get(13).copied().unwrap_or(f64::NAN),
                defenses: (14..v.len()).step_by(2).map(xy).collect(),
            }),
            9 => {
                r.starting_main = (at(0) >= 0).then_some(xy(0));
                r.starting_natural = (at(2) >= 0).then_some(xy(2));
            }
            10 => {
                let mut rest = &v[..];
                while let Some((&len, tail)) = rest.split_first() {
                    let len = (len.max(0.0) as usize).min(tail.len());
                    r.paths
                        .push(tail[..len].iter().map(|&i| i as i32).collect());
                    rest = &tail[len..];
                }
            }
            13 => {
                let mut pieces: [Vec<(i32, i32)>; 4] = Default::default();
                let mut i = 3;
                for list in &mut pieces {
                    let n = at(i).max(0) as usize;
                    *list = (0..n).map(|k| xy(i + 1 + 2 * k)).collect();
                    i += 1 + 2 * n;
                }
                let rows = at(i).max(0) as usize;
                i += 1;
                let mut defenses = Vec::new();
                for _ in 0..rows {
                    let (row, n) = (at(i), at(i + 1).max(0) as usize);
                    defenses.push((row, (0..n).map(|k| xy(i + 2 + 2 * k)).collect()));
                    i += 2 + 2 * n;
                }
                r.wall = Some(WallRecord {
                    defense_angle: v.first().copied().unwrap_or(f64::NAN),
                    choke: (at(1) >= 0).then_some(at(1)),
                    area: (at(2) >= 0).then_some(at(2)),
                    pieces,
                    defenses,
                });
            }
            12 => {
                let mut slots: [Vec<(i32, i32)>; 4] = Default::default();
                let mut i = 9;
                for list in &mut slots {
                    let n = at(i).max(0) as usize;
                    *list = (0..n).map(|k| xy(i + 1 + 2 * k)).collect();
                    i += 1 + 2 * n;
                }
                r.blocks.push(BlockRecord {
                    tile: xy(0),
                    size: xy(2),
                    center: xy(4),
                    proxy: at(6) == 1,
                    defensive: at(7) == 1,
                    kind: at(8),
                    slots,
                });
            }
            15 => {
                let tiles = (v.len().saturating_sub(1)) / 2;
                r.snapshots.push(Snapshot {
                    used: v[1..1 + tiles]
                        .iter()
                        .map(|&x| (x >= 0.0).then_some(x as i32))
                        .collect(),
                    reserved: v[1 + tiles..].iter().map(|&x| x != 0.0).collect(),
                });
            }
            11 => r.searches.push(SearchRecord {
                reachable: at(0) == 1,
                distance: v.get(1).copied().unwrap_or(-1.0),
                tiles: (2..v.len()).step_by(2).map(xy).collect(),
            }),
            14 => r.seconds = v,
            _ => return None,
        }
    }
    Some(r)
}
