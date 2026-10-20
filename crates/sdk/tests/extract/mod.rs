//! The records of `tests/property`, read from the port: what the checks take,
//! apart from the checks. `openbw-host` reads the same records from C++.

#![allow(dead_code)]

use bwapi::{Position, TilePosition, UnitType};
use bwapi_sdk::adapter::footprint;
use bwapi_sdk::bweb::{self, Blocks, Path, Pathfinding, Stations, Walls};
use bwapi_sdk::bwem::{Map, Node};

use crate::property::bweb::{
    Block, Navigation, Plan, Search, Snapshot, Station, Wall, search_cost, size_class,
};
use crate::property::bwem::{self, Analysis};

/// What the analysis of the port gives.
pub fn analysis(map: &Map, bases_found: bool) -> Analysis {
    let xy = |x: i32, y: i32| (x, y);
    let mut cps: Vec<_> = map.areas().flat_map(|a| a.choke_points()).collect();
    cps.sort_by_key(|cp| cp.id());
    cps.dedup_by_key(|cp| cp.id());
    Analysis {
        size: (map.size().x, map.size().y),
        max_altitude: i32::from(map.max_altitude()),
        altitudes: map.mini_tiles().map(|m| i32::from(m.altitude())).collect(),
        mini_tile_areas: map.mini_tiles().map(|m| i32::from(m.area_id())).collect(),
        tile_areas: map.tiles().map(|t| i32::from(t.area_id())).collect(),
        areas: map
            .areas()
            .map(|a| bwem::Area {
                id: i32::from(a.id().get()),
                top: xy(a.top().x, a.top().y),
                mini_tiles: a.size_in_mini_tiles(),
                group: i32::from(a.group()),
                top_left: xy(a.bounds().0.x, a.bounds().0.y),
                bottom_right: xy(a.bounds().1.x, a.bounds().1.y),
            })
            .collect(),
        choke_points: cps
            .iter()
            .map(|cp| {
                let (a, b) = cp.areas();
                let node = |n| {
                    let w = cp.node(n);
                    (w.x, w.y)
                };
                bwem::ChokePoint {
                    index: cp.id().index() as i32,
                    areas: (i32::from(a.id().get()), i32::from(b.id().get())),
                    nodes: [node(Node::End1), node(Node::Middle), node(Node::End2)],
                    pseudo: cp.is_pseudo(),
                    blocked: cp.is_blocked(),
                    geometry: cp.geometry().iter().map(|w| (w.x, w.y)).collect(),
                }
            })
            .collect(),
        bases: map
            .areas()
            .flat_map(|a| a.bases())
            .map(|b| bwem::Base {
                area: i32::from(b.area().id().get()),
                location: (b.location().x, b.location().y),
                starting: b.is_starting(),
                minerals: b.minerals().map(|m| m.unit_id()).collect(),
                geysers: b.geysers().map(|g| g.unit_id()).collect(),
                blocking_minerals: b.blocking_minerals().map(|m| m.unit_id()).collect(),
            })
            .collect(),
        bases_found,
        distances: cps
            .iter()
            .flat_map(|a| cps.iter().map(move |b| a.distance_to(b.id()).unwrap_or(-1)))
            .collect(),
        // An unreachable choke point has an empty path.
        paths: cps
            .iter()
            .flat_map(|a| {
                cps.iter().map(move |b| {
                    a.path_to(b.id())
                        .map(|p| p.iter().map(|cp| cp.id().index() as i32).collect())
                        .unwrap_or_default()
                })
            })
            .collect(),
    }
}

/// The used and reserved tiles of a layout, row by row.
pub fn snapshot(layout: &bweb::Layout) -> Snapshot {
    let size = layout.size();
    let tiles = || (0..size.y).flat_map(move |y| (0..size.x).map(move |x| TilePosition::new(x, y)));
    Snapshot {
        used: tiles().map(|t| layout.used_by(t, 1, 1)).collect(),
        reserved: tiles().map(|t| layout.is_reserved(t, 1, 1)).collect(),
    }
}

fn xy(t: &TilePosition) -> (i32, i32) {
    (t.x, t.y)
}

/// What BWEB of the port made: its stations, wall, blocks and the layout at
/// each stage.
pub fn plan(
    bwem: &Map,
    stations: &Stations,
    walls: &Walls,
    blocks: &Blocks,
    snapshots: Vec<Snapshot>,
    race: i32,
    start: TilePosition,
) -> Plan {
    let location = |s: &bweb::Station| xy(&bwem.base(s.base()).location());
    Plan {
        race,
        start: xy(&start),
        stations: stations
            .iter()
            .map(|s| Station {
                location: location(s),
                main: s.is_main(),
                natural: s.is_natural(),
                choke: s.choke_point().map(|cp| cp.index() as i32),
                small: s.small_slot().as_ref().map(xy),
                medium: s.medium_slot().as_ref().map(xy),
                defenses: s.defenses().iter().map(xy).collect(),
            })
            .collect(),
        starting_main: stations.own_main().map(location),
        starting_natural: stations.own_natural().map(location),
        wall: walls.iter().next().map(|w| Wall {
            choke: Some(w.choke_point().index() as i32),
            area: Some(i32::from(w.area().get())),
            small: w.small_tiles().iter().map(xy).collect(),
            medium: w.medium_tiles().iter().map(xy).collect(),
            large: w.large_tiles().iter().map(xy).collect(),
            openings: w.openings().iter().map(xy).collect(),
            defenses: (1..=3)
                .filter(|&row| !w.defenses(row).is_empty())
                .map(|row| (row, w.defenses(row).iter().map(xy).collect()))
                .collect(),
        }),
        blocks: blocks
            .iter()
            .map(|b| Block {
                tile: xy(&b.tile()),
                size: (b.width(), b.height()),
                kind: b.kind() as i32,
                small: b.small_tiles().iter().map(xy).collect(),
                medium: b.medium_tiles().iter().map(xy).collect(),
                large: b.large_tiles().iter().map(xy).collect(),
                wide: b.wide_tiles().iter().map(xy).collect(),
            })
            .collect(),
        snapshots,
        navigation: Navigation::default(),
    }
}

/// A unit of each size class `isWalkable` tells apart: full, small, medium,
/// large.
pub const WALK_TYPES: [UnitType; 4] = [
    UnitType::NONE,
    UnitType::ZERG_ZERGLING,
    UnitType::PROTOSS_PROBE,
    UnitType::PROTOSS_DRAGOON,
];

/// A small and a large unit for the searches of `Path`.
pub const PATH_TYPES: [UnitType; 2] = [UnitType::ZERG_ZERGLING, UnitType::PROTOSS_DRAGOON];

/// The bases searched between: every ordered pair of the first of them.
pub const PATH_BASES: usize = 8;

/// The mode of the searches over `unitWalkable`; the modes before it search
/// the terrain.
pub const UNIT_MODE: i32 = 3;

/// The tiles the searches of `Path` run between: the locations of the first
/// bases.
pub fn path_tiles(bwem: &Map) -> Vec<TilePosition> {
    bwem.areas()
        .flat_map(|a| a.bases())
        .take(PATH_BASES)
        .map(|b| b.location())
        .collect()
}

/// Base centers, start locations, choke point centers, the center of the map
/// and a point outside it: what the ground distances and closest choke tiles
/// are asked between and from.
pub fn points(bwem: &Map) -> Vec<Position> {
    let mut cps: Vec<_> = bwem.areas().flat_map(|a| a.choke_points()).collect();
    cps.sort_by_key(|cp| cp.id());
    cps.dedup_by_key(|cp| cp.id());
    bwem.areas()
        .flat_map(|a| a.bases())
        .map(|b| b.center())
        .chain(bwem.starting_locations().iter().map(|&t| Position::from(t)))
        .chain(cps.iter().map(|cp| Position::from(cp.center())))
        .chain([bwem.center(), Position::new(-1, -1)])
        .collect()
}

/// What BWEB of the port answers on `layout`: the walkability grids, the
/// searches in the order of `bweb_parity` (the terrain modes and the
/// distances and closest tiles only where `shared`), as one sequence with one
/// cache.
pub fn navigation(bwem: &Map, layout: &bweb::Layout, shared: bool) -> Navigation {
    let size = layout.size();
    let tiles = || (0..size.y).flat_map(move |y| (0..size.x).map(move |x| TilePosition::new(x, y)));
    let walkable = WALK_TYPES
        .iter()
        .map(|&ty| {
            let fp = footprint(ty);
            tiles().map(|t| layout.is_walkable(t, &fp)).collect()
        })
        .collect();

    let path_tiles = path_tiles(bwem);
    let first_mode = if shared { 0 } else { UNIT_MODE };
    let mut pathfinding = Pathfinding::new(layout);
    let cost = |_: &Path, t: TilePosition| search_cost((t.x, t.y));
    let terrain = |path: &Path, t: TilePosition| path.is_terrain_walkable(layout, t);
    let unit = |path: &Path, t: TilePosition| path.is_unit_walkable(layout, t);
    let mut searches = Vec::new();
    for ty in PATH_TYPES {
        let fp = footprint(ty);
        for &source in &path_tiles {
            for &target in &path_tiles {
                for mode in first_mode..=UNIT_MODE {
                    let mut path = Path::new(source, target, fp);
                    match mode {
                        0 => path.astar(&mut pathfinding, layout, terrain),
                        1 => path.astar_with_cost(&mut pathfinding, layout, cost, terrain),
                        2 => path.jps(&mut pathfinding, layout, terrain),
                        _ => path.jps(&mut pathfinding, layout, unit),
                    }
                    searches.push(Search {
                        class: size_class(fp.width, fp.height),
                        mode,
                        source: xy(&source),
                        target: xy(&target),
                        reachable: path.is_reachable(),
                        distance: path.distance(),
                        tiles: path.tiles().iter().map(xy).collect(),
                    });
                }
            }
        }
    }

    let mut nav = Navigation {
        walkable,
        searches,
        ..Navigation::default()
    };
    if shared {
        let points = points(bwem);
        let mut cps: Vec<_> = bwem.areas().flat_map(|a| a.choke_points()).collect();
        cps.sort_by_key(|cp| cp.id());
        cps.dedup_by_key(|cp| cp.id());
        for &a in &points {
            for &b in &points {
                nav.ground_distances
                    .push(layout.ground_distance_via_choke_points(bwem, a, b));
                // No choke point and no length when unreachable.
                let path = bwem.choke_point_path(a, b);
                nav.choke_paths.push(
                    path.map(|p| p.iter().map(|cp| cp.id().index() as i32).collect())
                        .unwrap_or_default(),
                );
                nav.bwem_lengths.push(path.and_then(|p| p.length()));
            }
        }
        nav.closest_choke_tiles = cps
            .iter()
            .map(|&cp| {
                let found = points
                    .iter()
                    .map(|&p| layout.closest_choke_point_tile(cp, p).map(|t| (t.x, t.y)))
                    .collect();
                (cp.id().index() as i32, found)
            })
            .collect();
        nav.points = points.iter().map(|p| (p.x, p.y)).collect();
    }
    nav
}
