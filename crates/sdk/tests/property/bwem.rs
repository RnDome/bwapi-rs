//! Properties of the analysis of BWEM: areas, choke points and bases, checked
//! against what the game reports of the map ([`Terrain`]).
//!
//! After a neutral is destroyed BWEM recomputes the areas of the mini tiles and
//! tiles under it and the resources of bases. Without automatic path update it
//! keeps the rest as the analysis computed it: the statistics of the areas
//! (their mini tiles, bounding box, top), whether choke points are blocked, the
//! groups, the distances and paths. [`When::AfterDestruction`] checks what is
//! recomputed only.

use std::collections::{BTreeMap, BTreeSet};

use bwapi_sdk::bwem::{MapInput, NeutralKind};

use super::{Offenders, Violation, edt, fill4};

/// What an analysis gives, in plain records. Grids row by row; positions as
/// `(x, y)`, of mini tiles where named so, else of tiles.
#[derive(Clone, Debug, Default)]
pub struct Analysis {
    /// In tiles.
    pub size: (i32, i32),
    pub max_altitude: i32,
    /// Of every mini tile.
    pub altitudes: Vec<i32>,
    /// Of every mini tile: 0 none, > 0 an area, < 0 a zone in no area.
    pub mini_tile_areas: Vec<i32>,
    /// Of every tile: the area of its mini tiles, -1 if they differ, 0 none.
    pub tile_areas: Vec<i32>,
    pub areas: Vec<Area>,
    pub choke_points: Vec<ChokePoint>,
    pub bases: Vec<Base>,
    /// Every start location got a base.
    pub bases_found: bool,
    /// The ground distance between every two choke points, by index, row by
    /// row: in pixels, -1 when unreachable.
    pub distances: Vec<i32>,
    /// The path between every two choke points, by index, row by row: the
    /// indices of the choke points from the one to the other, both included;
    /// empty when unreachable.
    pub paths: Vec<Vec<i32>>,
}

#[derive(Clone, Debug)]
pub struct Area {
    pub id: i32,
    /// A mini tile.
    pub top: (i32, i32),
    pub mini_tiles: i32,
    pub group: i32,
    pub top_left: (i32, i32),
    pub bottom_right: (i32, i32),
}

#[derive(Clone, Debug)]
pub struct ChokePoint {
    pub index: i32,
    pub areas: (i32, i32),
    /// Mini tiles: an end, the middle, the other end.
    pub nodes: [(i32, i32); 3],
    pub pseudo: bool,
    pub blocked: bool,
    /// The mini tiles of the frontier from one end to the other; empty where
    /// the record does not have them.
    pub geometry: Vec<(i32, i32)>,
}

#[derive(Clone, Debug)]
pub struct Base {
    pub area: i32,
    /// The top left tile of the resource depot.
    pub location: (i32, i32),
    pub starting: bool,
    /// Unit ids.
    pub minerals: Vec<i32>,
    pub geysers: Vec<i32>,
    /// Tiny mineral fields in the way of the depot, to mine out before it is
    /// built: `Base::BlockingMinerals`.
    pub blocking_minerals: Vec<i32>,
}

/// What the game reports of the map, less the neutrals destroyed.
#[derive(Clone, Debug)]
pub struct Terrain {
    /// In tiles.
    pub size: (i32, i32),
    /// `isWalkable` of every mini tile.
    pub walkable: Vec<bool>,
    pub buildable: Vec<bool>,
    pub neutrals: Vec<Neutral>,
    pub start_locations: Vec<(i32, i32)>,
}

#[derive(Clone, Debug)]
pub struct Neutral {
    pub unit_id: i32,
    pub type_id: i32,
    pub kind: NeutralKind,
    /// The top left tile as BWEM takes it: one tile right for a right pit door.
    pub top_left: (i32, i32),
    /// `initial_tile_position`, as the game and BWEB have it.
    pub tile: (i32, i32),
    pub size: (i32, i32),
    pub amount: i32,
    pub building: bool,
    /// It does not move: BWEB reserves its tiles.
    pub immobile: bool,
}

impl Terrain {
    /// The terrain of `input` without the neutrals of the unit ids `destroyed`.
    pub fn new(input: &MapInput, destroyed: &[i32]) -> Terrain {
        Terrain {
            size: (input.buildable.width(), input.buildable.height()),
            walkable: input.walkable.cells().to_vec(),
            buildable: input.buildable.cells().to_vec(),
            neutrals: input
                .neutrals
                .iter()
                .filter(|n| !destroyed.contains(&n.unit_id))
                .map(|n| Neutral {
                    unit_id: n.unit_id,
                    type_id: n.type_id,
                    kind: n.kind,
                    top_left: (n.top_left.x, n.top_left.y),
                    tile: (n.tile_position.x, n.tile_position.y),
                    size: (n.size.x, n.size.y),
                    amount: n.initial_amount,
                    building: n.building,
                    immobile: n.immobile,
                })
                .collect(),
            start_locations: input.start_locations.iter().map(|t| (t.x, t.y)).collect(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum When {
    Analysis,
    AfterDestruction,
}

/// A resource depot, in tiles.
const DEPOT: (i32, i32) = (4, 3);

/// The tiles from a resource depot no mineral field or geyser may be on:
/// `canBuildHere` of BWAPI forbids it.
const RESOURCE_GAP: i32 = 3;

/// Mineral fields of at most this much do not keep a depot away: BWEM gives
/// them to the base to mine out (`ValidateBaseLocation`).
const TINY_MINERALS: i32 = 8;

/// A bounding box: its top left and bottom right.
type Bounds = ((i32, i32), (i32, i32));

/// The violations of `a`, an analysis of `t`.
pub fn check(a: &Analysis, t: &Terrain, when: When) -> Vec<Violation> {
    let mut out = Vec::new();
    let mut fail = |property: &'static str, subject: String, detail: String| {
        out.push(Violation {
            property: property.to_owned(),
            subject,
            detail,
        });
    };
    let walk = (a.size.0 * 4, a.size.1 * 4);
    let mini = |x: i32, y: i32| a.mini_tile_areas[(y * walk.0 + x) as usize];

    // Areas: ids 1..=N, each once.
    let ids: BTreeSet<i32> = a.areas.iter().map(|area| area.id).collect();
    let n = a.areas.len() as i32;
    if ids.len() != a.areas.len() || ids.iter().any(|&id| !(1..=n).contains(&id)) {
        fail("area_ids", "areas".into(), format!("ids {ids:?}"));
    }

    // One pass over the mini tiles of areas: by area, their count, highest
    // altitude and parts, 4-connected, as BWEM grows areas by 4 neighbours
    // (`ComputeTempAreas`, `ReplaceAreaIds`): the size and a mini tile of each.
    #[derive(Default)]
    struct Stats {
        count: i32,
        highest: i32,
        parts: Vec<(i32, (i32, i32))>,
    }
    let mut stats: BTreeMap<i32, Stats> = BTreeMap::new();
    let mut seen = vec![false; a.mini_tile_areas.len()];
    for y in 0..walk.1 {
        for x in 0..walk.0 {
            let i = (y * walk.0 + x) as usize;
            let id = a.mini_tile_areas[i];
            if id <= 0 {
                continue;
            }
            let s = stats.entry(id).or_default();
            s.count += 1;
            s.highest = s.highest.max(a.altitudes[i]);
            if !seen[i] {
                let size = fill4(&mut seen, walk.0, walk.1, (x, y), |x, y| mini(x, y) == id);
                s.parts.push((size, (x, y)));
            }
        }
    }

    // Every mini tile of an area id belongs to an existing area.
    for (&id, s) in &stats {
        if !ids.contains(&id) {
            fail(
                "mini_tile_area_exists",
                format!("area {id}"),
                format!("{} mini tiles", s.count),
            );
        }
    }

    // The area of a tile is that of its mini tiles of an area or zone: theirs
    // if they share it, -1 if they differ, 0 if none has one.
    for ty in 0..a.size.1 {
        for tx in 0..a.size.0 {
            let mut expected = 0;
            for dy in 0..4 {
                for dx in 0..4 {
                    let id = mini(tx * 4 + dx, ty * 4 + dy);
                    if id != 0 {
                        expected = match expected {
                            0 => id,
                            e if e == id => e,
                            _ => -1,
                        };
                    }
                    if expected == -1 {
                        break;
                    }
                }
            }
            let got = a.tile_areas[(ty * a.size.0 + tx) as usize];
            if got != expected {
                fail(
                    "tile_area",
                    format!("tile {tx},{ty}"),
                    format!("{got}, its mini tiles give {expected}"),
                );
            }
        }
    }

    // Each area is one part.
    for area in &a.areas {
        let Some(s) = stats.get(&area.id) else {
            fail(
                "area_mini_tiles",
                format!("area {}", area.id),
                "no mini tile".into(),
            );
            continue;
        };
        if s.parts.len() > 1 {
            let largest = s.parts.iter().map(|p| p.0).max().unwrap_or(0);
            let others: Vec<(i32, i32)> = s
                .parts
                .iter()
                .filter(|p| p.0 != largest)
                .map(|p| p.1)
                .collect();
            fail(
                "bwem_area_connected4",
                format!("area {}", area.id),
                format!(
                    "{} parts, {} of its {} mini tiles apart from the largest, at {:?}",
                    s.parts.len(),
                    s.count - largest,
                    s.count,
                    &others[..others.len().min(4)]
                ),
            );
        }
    }

    if when == When::Analysis {
        // The bounding boxes of the tiles of each area.
        let mut bounds: BTreeMap<i32, Bounds> = BTreeMap::new();
        for ty in 0..a.size.1 {
            for tx in 0..a.size.0 {
                let id = a.tile_areas[(ty * a.size.0 + tx) as usize];
                if id > 0 {
                    let b = bounds.entry(id).or_insert(((tx, ty), (tx, ty)));
                    b.0 = (b.0.0.min(tx), b.0.1.min(ty));
                    b.1 = (b.1.0.max(tx), b.1.1.max(ty));
                }
            }
        }
        for area in &a.areas {
            let subject = format!("area {}", area.id);
            let (count, highest) = stats
                .get(&area.id)
                .map_or((0, None), |s| (s.count, Some(s.highest)));
            if area.mini_tiles != count {
                fail(
                    "area_mini_tiles",
                    subject.clone(),
                    format!("{}, counted {count}", area.mini_tiles),
                );
            }
            // The top is a mini tile of the area of its highest altitude
            // (`Area::Top`).
            let (x, y) = area.top;
            let inside = (0..walk.0).contains(&x) && (0..walk.1).contains(&y);
            if !inside
                || mini(x, y) != area.id
                || Some(a.altitudes[(y * walk.0 + x) as usize]) != highest
            {
                fail(
                    "area_top",
                    subject.clone(),
                    format!("top {x},{y}, highest altitude {highest:?}"),
                );
            }
            // The bounding box is that of the tiles of the area.
            if let Some(&b) = bounds.get(&area.id)
                && b != (area.top_left, area.bottom_right)
            {
                fail(
                    "area_bounding_box",
                    subject,
                    format!(
                        "{:?} {:?}, its tiles span {:?} {:?}",
                        area.top_left, area.bottom_right, b.0, b.1
                    ),
                );
            }
        }
    }

    // Choke points, groups, distances, paths and altitudes: only as the
    // analysis computed them, as BWEM keeps them after a destruction.
    if when == When::Analysis {
        check_choke_points(a, &ids, &mut fail);
        check_altitudes(a, t, &mut fail);
    }
    check_bases(a, t, &ids, when, &mut fail);
    out
}

/// Choke points: each index once, between two different existing areas, its
/// nodes on the map; the middle of a frontier in one of its areas. Groups are
/// the areas reachable through the choke points that are not blocked.
fn check_choke_points(
    a: &Analysis,
    ids: &BTreeSet<i32>,
    fail: &mut impl FnMut(&'static str, String, String),
) {
    let walk = (a.size.0 * 4, a.size.1 * 4);
    let mini = |x: i32, y: i32| a.mini_tile_areas[(y * walk.0 + x) as usize];
    let mut indices = BTreeSet::new();
    for cp in &a.choke_points {
        let subject = format!("choke point {}", cp.index);
        if !indices.insert(cp.index) {
            fail("choke_point_index", subject.clone(), "twice".into());
        }
        let (x, y) = cp.areas;
        if x == y || !ids.contains(&x) || !ids.contains(&y) {
            fail(
                "choke_point_areas",
                subject.clone(),
                format!("areas {x} {y}"),
            );
        }
        for &(nx, ny) in &cp.nodes {
            if !(0..walk.0).contains(&nx) || !(0..walk.1).contains(&ny) {
                fail(
                    "choke_point_on_map",
                    subject.clone(),
                    format!("node {nx},{ny}"),
                );
            }
        }
        let (mx, my) = cp.nodes[1];
        if !cp.pseudo
            && (0..walk.0).contains(&mx)
            && (0..walk.1).contains(&my)
            && mini(mx, my) != x
            && mini(mx, my) != y
        {
            fail(
                "choke_point_middle",
                subject,
                format!("middle {mx},{my} in {}, not {x} or {y}", mini(mx, my)),
            );
        }
    }

    // Groups are the areas reachable through choke points that are not
    // blocked.
    let mut parent: BTreeMap<i32, i32> = ids.iter().map(|&id| (id, id)).collect();
    fn root(parent: &mut BTreeMap<i32, i32>, id: i32) -> i32 {
        let mut r = id;
        while parent[&r] != r {
            r = parent[&r];
        }
        parent.insert(id, r);
        r
    }
    for cp in a.choke_points.iter().filter(|cp| !cp.blocked) {
        let (x, y) = cp.areas;
        if parent.contains_key(&x) && parent.contains_key(&y) {
            let (rx, ry) = (root(&mut parent, x), root(&mut parent, y));
            parent.insert(rx, ry);
        }
    }
    let group_of: BTreeMap<i32, i32> = a.areas.iter().map(|area| (area.id, area.group)).collect();
    for x in ids {
        for y in ids.iter().filter(|&y| y > x) {
            let connected = root(&mut parent, *x) == root(&mut parent, *y);
            let grouped = group_of[x] == group_of[y];
            if connected != grouped {
                fail(
                    "area_groups",
                    format!("areas {x} {y}"),
                    format!(
                        "connected {connected}, groups {} {}",
                        group_of[x], group_of[y]
                    ),
                );
            }
        }
    }

    check_distances(a, &group_of, fail);
}

/// Bases: on the map, buildable, free but for tiny minerals to mine out, away
/// from resources, with resources that exist and are of one base; start
/// locations on starting bases.
fn check_bases(
    a: &Analysis,
    t: &Terrain,
    ids: &BTreeSet<i32>,
    when: When,
    fail: &mut impl FnMut(&'static str, String, String),
) {
    let tile_ok = |x: i32, y: i32| (0..t.size.0).contains(&x) && (0..t.size.1).contains(&y);
    let mut owner: BTreeMap<i32, (i32, i32)> = BTreeMap::new();
    for b in &a.bases {
        let (bx, by) = b.location;
        let subject = format!("base {bx},{by}");
        if !ids.contains(&b.area) {
            fail("base_area", subject.clone(), format!("area {}", b.area));
        }
        let footprint: Vec<(i32, i32)> = (0..DEPOT.1)
            .flat_map(|dy| (0..DEPOT.0).map(move |dx| (bx + dx, by + dy)))
            .collect();
        if footprint.iter().any(|&(x, y)| !tile_ok(x, y)) {
            fail("base_on_map", subject.clone(), String::new());
            continue;
        }
        if let Some(&(x, y)) = footprint
            .iter()
            .find(|&&(x, y)| !t.buildable[(y * t.size.0 + x) as usize])
        {
            fail("base_buildable", subject.clone(), format!("tile {x},{y}"));
        }
        // The tiles of the depot are of its area: where `ComputeBaseLocationScore`
        // places it. Not after a destruction: tiles change areas.
        if when == When::Analysis
            && let Some(&(x, y)) = footprint
                .iter()
                .find(|&&(x, y)| a.tile_areas[(y * a.size.0 + x) as usize] != b.area)
        {
            fail(
                "bwem_base_in_its_area",
                subject.clone(),
                format!("tile {x},{y}"),
            );
        }
        for n in &t.neutrals {
            let (nx, ny) = n.top_left;
            let (nw, nh) = n.size;
            let overlaps = |gap: i32| {
                nx < bx + DEPOT.0 + gap
                    && bx - gap < nx + nw
                    && ny < by + DEPOT.1 + gap
                    && by - gap < ny + nh
            };
            let resource = match n.kind {
                NeutralKind::Geyser => true,
                NeutralKind::Mineral => n.amount > TINY_MINERALS,
                _ => false,
            };
            let to_mine_out = n.kind == NeutralKind::Mineral
                && n.amount <= TINY_MINERALS
                && b.blocking_minerals.contains(&n.unit_id);
            if n.kind != NeutralKind::Other && overlaps(0) && !to_mine_out {
                fail(
                    "base_free",
                    subject.clone(),
                    format!("unit {} on it", n.unit_id),
                );
            } else if resource && overlaps(RESOURCE_GAP) {
                fail(
                    "base_away_from_resources",
                    subject.clone(),
                    format!("unit {} within {RESOURCE_GAP} tiles", n.unit_id),
                );
            }
        }
        // A mineral to mine out before the depot is built is a tiny one in its
        // way, listed once.
        let mut listed = BTreeSet::new();
        for &id in &b.blocking_minerals {
            if !listed.insert(id) {
                fail(
                    "base_blocking_mineral_once",
                    subject.clone(),
                    format!("unit {id}"),
                );
            }
            let in_way = t
                .neutrals
                .iter()
                .find(|n| n.unit_id == id)
                .is_some_and(|n| {
                    let (nx, ny) = n.top_left;
                    let (nw, nh) = n.size;
                    n.kind == NeutralKind::Mineral
                        && n.amount <= TINY_MINERALS
                        && nx < bx + DEPOT.0 + RESOURCE_GAP
                        && bx - RESOURCE_GAP < nx + nw
                        && ny < by + DEPOT.1 + RESOURCE_GAP
                        && by - RESOURCE_GAP < ny + nh
                });
            if !in_way {
                fail(
                    "base_blocking_mineral",
                    subject.clone(),
                    format!("unit {id} is no tiny mineral in the way of the depot"),
                );
            }
        }
        let resources = b
            .minerals
            .iter()
            .map(|&id| (id, NeutralKind::Mineral))
            .chain(b.geysers.iter().map(|&id| (id, NeutralKind::Geyser)));
        for (id, kind) in resources {
            if !t.neutrals.iter().any(|n| n.unit_id == id && n.kind == kind) {
                fail(
                    "base_resource_exists",
                    subject.clone(),
                    format!("{kind:?} of unit {id}"),
                );
            }
            if let Some(other) = owner.insert(id, b.location) {
                fail(
                    "base_resource_once",
                    subject.clone(),
                    format!("unit {id} also of base {},{}", other.0, other.1),
                );
            }
        }
    }

    // Every start location has one starting base on it, if all got one; a
    // starting base is on a start location.
    for b in a.bases.iter().filter(|b| b.starting) {
        if !t.start_locations.contains(&b.location) {
            fail(
                "base_starting",
                format!("base {},{}", b.location.0, b.location.1),
                "on no start location".into(),
            );
        }
    }
    if a.bases_found {
        for &s in &t.start_locations {
            let on = a
                .bases
                .iter()
                .filter(|b| b.starting && b.location == s)
                .count();
            if on != 1 {
                fail(
                    "start_location_base",
                    format!("start {},{}", s.0, s.1),
                    format!("{on} starting bases on it"),
                );
            }
        }
    }
}

/// The distances and paths between choke points are those of shortest paths
/// through areas, where a blocked choke point may end a path but is not passed
/// through; a choke point reaches another when an area of the one shares a
/// group with an area of the other. Paths both ways are each checked; BWEM
/// also stores the one as the other reversed (`Graph::SetPath`).
fn check_distances(
    a: &Analysis,
    group_of: &BTreeMap<i32, i32>,
    fail: &mut impl FnMut(&'static str, String, String),
) {
    let n = a.choke_points.len();
    let by_index: BTreeMap<i32, &ChokePoint> =
        a.choke_points.iter().map(|cp| (cp.index, cp)).collect();
    if !by_index.keys().copied().eq(0..n as i32) {
        fail(
            "choke_point_indices",
            "choke points".into(),
            format!("indices {:?}", by_index.keys().collect::<Vec<_>>()),
        );
        return;
    }
    if a.distances.len() != n * n || a.paths.len() != n * n {
        fail(
            "choke_point_matrices",
            "choke points".into(),
            format!(
                "{} distances, {} paths for {n} choke points",
                a.distances.len(),
                a.paths.len()
            ),
        );
        return;
    }
    let cp = |i: i32| by_index[&i];
    let d = |i: usize, j: usize| a.distances[i * n + j];
    let path = |i: usize, j: usize| &a.paths[i * n + j];
    let shared_area = |x: &ChokePoint, y: &ChokePoint| {
        let (p, q) = x.areas;
        let (r, s) = y.areas;
        p == r || p == s || q == r || q == s
    };
    let grouped = |x: &ChokePoint, y: &ChokePoint| {
        [x.areas.0, x.areas.1].iter().any(|ax| {
            [y.areas.0, y.areas.1]
                .iter()
                .any(|ay| group_of.get(ax).is_some() && group_of.get(ax) == group_of.get(ay))
        })
    };

    for i in 0..n {
        if d(i, i) != 0 || path(i, i) != &[i as i32] {
            fail(
                "choke_point_to_itself",
                format!("choke point {i}"),
                format!("distance {}, path {:?}", d(i, i), path(i, i)),
            );
        }
        for j in (0..n).filter(|&j| j != i) {
            let subject = || format!("choke points {i} {j}");
            let (dij, p) = (d(i, j), path(i, j));
            if i < j {
                if dij != d(j, i) {
                    fail(
                        "choke_point_distance_symmetric",
                        subject(),
                        format!("{dij}, back {}", d(j, i)),
                    );
                }
                if !p.iter().rev().eq(path(j, i).iter()) {
                    fail(
                        "bwem_choke_point_path_mirrored",
                        subject(),
                        format!("{p:?}, back {:?}", path(j, i)),
                    );
                }
                let reachable = dij >= 0;
                if reachable != grouped(cp(i as i32), cp(j as i32)) {
                    fail(
                        "choke_point_reachable_in_group",
                        subject(),
                        format!(
                            "distance {dij}, areas {:?} {:?}",
                            cp(i as i32).areas,
                            cp(j as i32).areas
                        ),
                    );
                }
            }
            if dij < -1 || (dij == -1) != p.is_empty() {
                fail(
                    "choke_point_path_iff_reachable",
                    subject(),
                    format!("distance {dij}, path {p:?}"),
                );
                continue;
            }
            if p.is_empty() {
                continue;
            }
            if p.iter().any(|&k| !(0..n as i32).contains(&k)) {
                fail("choke_point_path_indices", subject(), format!("{p:?}"));
                continue;
            }
            if p[0] != i as i32 || *p.last().expect("a choke point") != j as i32 {
                fail("choke_point_path_ends", subject(), format!("{p:?}"));
            }
            if let Some(w) = p.windows(2).find(|w| !shared_area(cp(w[0]), cp(w[1]))) {
                fail(
                    "choke_point_path_steps",
                    subject(),
                    format!("{} to {} share no area in {p:?}", w[0], w[1]),
                );
            }
            if let Some(&k) = p[1..p.len() - 1].iter().find(|&&k| cp(k).blocked) {
                fail(
                    "choke_point_path_open",
                    subject(),
                    format!("through blocked {k} in {p:?}"),
                );
            }
            let steps: Vec<i32> = p
                .windows(2)
                .map(|w| d(w[0] as usize, w[1] as usize))
                .collect();
            if steps.contains(&-1) || steps.iter().sum::<i32>() != dij {
                fail(
                    "choke_point_path_length",
                    subject(),
                    format!("distance {dij}, steps {steps:?} along {p:?}"),
                );
            }
        }
    }

    // Shortest: no way through an open choke point is shorter.
    for i in 0..n {
        for j in i + 1..n {
            let via = (0..n)
                .filter(|&k| k != i && k != j && !cp(k as i32).blocked)
                .find(|&k| {
                    let (ik, kj) = (d(i, k), d(k, j));
                    ik >= 0 && kj >= 0 && (d(i, j) == -1 || d(i, j) > ik + kj)
                });
            if let Some(k) = via {
                fail(
                    "choke_point_shortest",
                    format!("choke points {i} {j}"),
                    format!("{} > {} via {k}", d(i, j), d(i, k) + d(k, j)),
                );
            }
        }
    }
}

/// Altitudes as `tiles.h` defines them: the distance in pixels from the center
/// of a mini tile to the center of the nearest sea mini tile, a sea mini tile
/// being an unwalkable one of altitude 0 and a lake one an unwalkable one of
/// altitude above 0; walkable is `AreaId() != 0`. The ring around the map is a
/// source as well: `ComputeAltitude` seeds it.
fn check_altitudes(a: &Analysis, t: &Terrain, fail: &mut impl FnMut(&'static str, String, String)) {
    let walk = (a.size.0 * 4, a.size.1 * 4);
    let at = |x: i32, y: i32| (y * walk.0 + x) as usize;
    let walkable = |x: i32, y: i32| a.mini_tile_areas[at(x, y)] != 0;
    let altitude = |x: i32, y: i32| a.altitudes[at(x, y)];
    let sea = |x: i32, y: i32| !walkable(x, y) && altitude(x, y) == 0;
    let cells = || (0..walk.1).flat_map(move |y| (0..walk.0).map(move |x| (x, y)));

    // In range: no point is farther from the ring than half the narrower side.
    let bound = 8 * (walk.0.min(walk.1) + 1) / 2 + 1;
    let mut range = Offenders::new("altitude_range", "mini tiles");
    for (x, y) in cells() {
        if !(0..=bound).contains(&altitude(x, y)) {
            range.add((x, y), altitude(x, y), bound, || {
                format!("{} outside 0..={bound}", altitude(x, y))
            });
        }
    }
    range.report(fail);

    let highest = a.altitudes.iter().copied().max().unwrap_or(0);
    if highest != a.max_altitude {
        fail(
            "max_altitude",
            "map".into(),
            format!("{}, the mini tiles reach {highest}", a.max_altitude),
        );
    }

    let mut not_sea = Offenders::new("altitude_walkable_not_sea", "mini tiles");
    for (x, y) in cells().filter(|&(x, y)| walkable(x, y) && altitude(x, y) == 0) {
        not_sea.add((x, y), 0, 1, || "walkable, altitude 0".into());
    }
    not_sea.report(fail);

    // Zones of unwalkable mini tiles, by 4 neighbours as `DecideSeasOrLakes`
    // makes them: each a sea or a lake as a whole, a lake exactly when small and
    // away from the edges.
    let mut seen = vec![false; a.altitudes.len()];
    for (x, y) in cells() {
        if walkable(x, y) || seen[at(x, y)] {
            continue;
        }
        let mut stack = vec![(x, y)];
        seen[at(x, y)] = true;
        let (mut size, mut seas) = (0, 0);
        let (mut top_left, mut bottom_right) = ((x, y), (x, y));
        while let Some((cx, cy)) = stack.pop() {
            size += 1;
            seas += i32::from(altitude(cx, cy) == 0);
            top_left = (top_left.0.min(cx), top_left.1.min(cy));
            bottom_right = (bottom_right.0.max(cx), bottom_right.1.max(cy));
            for (dx, dy) in [(0, -1), (-1, 0), (1, 0), (0, 1)] {
                let (nx, ny) = (cx + dx, cy + dy);
                if (0..walk.0).contains(&nx)
                    && (0..walk.1).contains(&ny)
                    && !walkable(nx, ny)
                    && !seen[at(nx, ny)]
                {
                    seen[at(nx, ny)] = true;
                    stack.push((nx, ny));
                }
            }
        }
        let subject = format!("zone at {x},{y}");
        if seas != 0 && seas != size {
            fail(
                "altitude_zone_sea_or_lake",
                subject,
                format!("{seas} of its {size} mini tiles of altitude 0"),
            );
            continue;
        }
        // `DecideSeasOrLakes`: `lake_max_miniTiles` 300, `lake_max_width_in_miniTiles`
        // 32 (`defs.h`), 2 mini tiles from the edges.
        let small = size <= 300
            && bottom_right.0 - top_left.0 <= 32
            && bottom_right.1 - top_left.1 <= 32
            && top_left.0 >= 2
            && top_left.1 >= 2
            && bottom_right.0 < walk.0 - 2
            && bottom_right.1 < walk.1 - 2;
        let lake = seas == 0;
        if small != lake {
            fail(
                "bwem_lake_rule",
                subject,
                format!(
                    "{} of {size} mini tiles from {top_left:?} to {bottom_right:?}",
                    if lake { "a lake" } else { "a sea" }
                ),
            );
        }
    }

    // Against the exact distance transform to the sea mini tiles and the ring,
    // rounded as BWEM rounds: `0.5 + 8 d` truncated. An altitude is the distance
    // to some sea, so never less than that to the nearest: a necessary
    // condition for every implementation. That it is the distance to the
    // nearest is the target; the detail tells a geometric deviation (more than
    // half a pixel) from one of rounding.
    let distances = edt::distances(walk.0, walk.1, sea, true);
    let mut at_least = Offenders::new("altitude_at_least_distance", "mini tiles");
    let mut equal = Offenders::new("altitude_equals_exact_distance", "mini tiles");
    let mut worst = 0.0f64;
    for (x, y) in cells().filter(|&(x, y)| !sea(x, y)) {
        let d = distances[at(x, y)].expect("the ring is a source");
        let exact = 8.0 * d;
        let expected = (0.5 + d * 8.0) as i32;
        let found = altitude(x, y);
        let detail = || {
            let kind = if (f64::from(found) - exact).abs() > 0.5 {
                "geometry"
            } else {
                "rounding"
            };
            format!("altitude {found}, 8 d = {exact:.3}, {kind}")
        };
        if found < expected {
            at_least.add((x, y), found, expected, detail);
        }
        if found != expected {
            worst = worst.max((f64::from(found) - exact).abs());
            equal.add((x, y), found, expected, detail);
        }
    }
    if let Some(first) = &mut equal.first {
        first.push_str(&format!("; deviation up to {worst:.3}"));
    }
    at_least.report(fail);
    equal.report(fail);

    // `LoadData`: a mini tile unwalkable in BWAPI makes its 8 neighbours
    // unwalkable, then every mini tile of a tile buildable or walkable all over
    // in BWAPI is walkable.
    let bwapi = |x: i32, y: i32| t.walkable[at(x, y)];
    let mut forced = Offenders::new("bwem_walkable_forced", "mini tiles");
    let mut eroded = Offenders::new("bwem_walkable_eroded", "mini tiles");
    for (x, y) in cells() {
        let (tx, ty) = (x / 4, y / 4);
        let tile_walkable = (0..4).all(|dy| (0..4).all(|dx| bwapi(tx * 4 + dx, ty * 4 + dy)));
        let whole = t.buildable[(ty * t.size.0 + tx) as usize] || tile_walkable;
        if whole {
            if !walkable(x, y) {
                forced.add((x, y), 0, 1, || {
                    "unwalkable in a tile buildable or walkable all over".into()
                });
            }
            continue;
        }
        let around = (-1..=1).all(|dy| {
            (-1..=1).all(|dx| {
                let (nx, ny) = (x + dx, y + dy);
                !(0..walk.0).contains(&nx) || !(0..walk.1).contains(&ny) || bwapi(nx, ny)
            })
        });
        if around != walkable(x, y) {
            eroded.add((x, y), i32::from(walkable(x, y)), i32::from(around), || {
                format!(
                    "walkable {}, its 3x3 walkable in BWAPI {around}",
                    walkable(x, y)
                )
            });
        }
    }
    forced.report(fail);
    eroded.report(fail);
}
