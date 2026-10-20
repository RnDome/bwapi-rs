//! BWEB on synthetic maps drawn tile by tile.

use bwapi::{Position, Race, TilePosition, UnitType, WalkPosition};
use bwapi_sdk::PositionExt;
use bwapi_sdk::bweb::{
    self, Blocks, Footprint, Path, Pathfinding, StartState, Stations, WallError, WallSpec, Walls,
    perpendicular_line,
};
use bwapi_sdk::bwem::{self, MapInput, NeutralInput, NeutralKind};
use bwapi_sdk::grid::Grid;

/// One character per tile: `.` buildable ground, `,` walkable unbuildable
/// ground, `#` unwalkable.
fn terrain(rows: &[&str]) -> MapInput {
    let (width, height) = (rows[0].len() as i32, rows.len() as i32);
    let at = |t: TilePosition| rows[t.y as usize].as_bytes()[t.x as usize];
    MapInput {
        walkable: Grid::from_fn(width * 4, height * 4, |w: WalkPosition| {
            at(TilePosition::from(w)) != b'#'
        }),
        buildable: Grid::from_fn(width, height, |t| at(t) == b'.'),
        ground_height: Grid::filled(width, height, 0),
        neutrals: Vec::new(),
        start_locations: Vec::new(),
    }
}

fn open(width: usize, height: usize) -> Vec<String> {
    vec![".".repeat(width); height]
}

fn rows(lines: &[String]) -> Vec<&str> {
    lines.iter().map(String::as_str).collect()
}

fn neutral(
    unit_id: i32,
    ty: UnitType,
    kind: NeutralKind,
    x: i32,
    y: i32,
    size: (i32, i32),
) -> NeutralInput {
    let top_left = TilePosition::new(x, y);
    let size = TilePosition::new(size.0, size.1);
    NeutralInput {
        unit_id,
        type_id: ty.id(),
        kind,
        top_left,
        size,
        pos: Position::from(top_left) + Position::from(size) / 2,
        initial_amount: if kind == NeutralKind::Geyser {
            5000
        } else {
            1500
        },
        tile_position: top_left,
        building: kind != NeutralKind::Other,
        immobile: true,
    }
}

fn mineral(unit_id: i32, x: i32, y: i32) -> NeutralInput {
    neutral(
        unit_id,
        UnitType::RESOURCE_MINERAL_FIELD,
        NeutralKind::Mineral,
        x,
        y,
        (2, 1),
    )
}

fn geyser(unit_id: i32, x: i32, y: i32) -> NeutralInput {
    neutral(
        unit_id,
        UnitType::RESOURCE_VESPENE_GEYSER,
        NeutralKind::Geyser,
        x,
        y,
        (4, 2),
    )
}

fn layout(input: &MapInput) -> (bwem::Map, bweb::Layout) {
    let mut bwem = bwem::Map::initialize(input).unwrap();
    bwem.find_bases_for_starting_locations();
    let layout = bweb::Layout::new(&bwem, input);
    (bwem, layout)
}

fn footprint(ty: UnitType, tiles: (i32, i32), pixels: (i32, i32)) -> Footprint {
    Footprint {
        type_id: ty.id(),
        tile_width: tiles.0,
        tile_height: tiles.1,
        width: pixels.0,
        height: pixels.1,
        requires_creep: false,
        is_resource_depot: false,
    }
}

/// The sizes of BWAPI: a zergling is small, a probe medium, a dragoon large.
const ZERGLING: (i32, i32) = (16, 16);
const PROBE: (i32, i32) = (23, 23);
const DRAGOON: (i32, i32) = (32, 32);

fn pylon() -> Footprint {
    footprint(UnitType::PROTOSS_PYLON, (2, 2), (32, 32))
}

fn spawning_pool() -> Footprint {
    Footprint {
        requires_creep: true,
        ..footprint(UnitType::ZERG_SPAWNING_POOL, (3, 2), (72, 48))
    }
}

fn command_center() -> Footprint {
    Footprint {
        is_resource_depot: true,
        ..footprint(UnitType::TERRAN_COMMAND_CENTER, (4, 3), (117, 83))
    }
}

#[test]
fn walkability_depends_on_the_size_of_the_unit() {
    // A corridor of one tile at x = 5 and one of two tiles at x = 10..12.
    let mut lines = vec!["#".repeat(16); 16];
    for line in &mut lines[2..14] {
        line.replace_range(5..6, ".");
        line.replace_range(10..12, "..");
    }
    let (_, layout) = layout(&terrain(&rows(&lines)));
    let walkable = |x, y, size: (i32, i32)| {
        let fp = Footprint {
            width: size.0,
            height: size.1,
            ..Footprint::NONE
        };
        layout.is_walkable(TilePosition::new(x, y), &fp)
    };

    // In the narrow corridor, only units up to small size pass.
    assert!(walkable(5, 8, (0, 0)));
    assert!(walkable(5, 8, ZERGLING));
    assert!(!walkable(5, 8, PROBE));
    assert!(!walkable(5, 8, DRAGOON));
    // In the wide one, every size.
    for x in 10..12 {
        for size in [(0, 0), ZERGLING, PROBE, DRAGOON] {
            assert!(walkable(x, 8, size), "({x}, 8), {size:?}");
        }
    }
    assert!(!walkable(0, 0, (0, 0)));
    assert!(!walkable(-1, 0, (0, 0)));
    assert!(!walkable(16, 0, (0, 0)));
}

#[test]
fn geysers_are_used_and_not_walkable() {
    let mut input = terrain(&rows(&open(20, 20)));
    input.neutrals.push(geyser(1, 8, 8));
    let (_, layout) = layout(&input);

    let geyser_id = Some(UnitType::RESOURCE_VESPENE_GEYSER.id());
    for x in 8..12 {
        for y in 8..10 {
            let t = TilePosition::new(x, y);
            assert!(!layout.is_walkable(t, &Footprint::NONE));
            assert_eq!(layout.used_by(t, 1, 1), geyser_id);
            assert!(layout.is_reserved(t, 1, 1));
        }
    }
    assert!(layout.is_walkable(TilePosition::new(12, 8), &Footprint::NONE));
    assert_eq!(layout.used_by(TilePosition::new(12, 8), 1, 1), None);
    // The first building found in a rectangle.
    assert_eq!(layout.used_by(TilePosition::new(6, 6), 4, 4), geyser_id);
}

#[test]
fn neutrals_are_used_if_buildings_and_reserved_if_immobile() {
    let mut input = terrain(&rows(&open(20, 20)));
    // A power-up that stays: reserved, not used.
    input.neutrals.push(neutral(
        1,
        UnitType::POWERUP_KHAYDARIN_CRYSTAL,
        NeutralKind::Other,
        2,
        2,
        (1, 1),
    ));
    // A critter: neither.
    input.neutrals.push(NeutralInput {
        immobile: false,
        ..neutral(
            2,
            UnitType::CRITTER_RHYNADON,
            NeutralKind::Other,
            5,
            5,
            (1, 1),
        )
    });
    input.neutrals.push(mineral(3, 10, 2));
    let (bwem, layout) = layout(&input);
    // BWEM keeps only the mineral.
    let count = |kind| bwem.neutrals().filter(|n| n.kind() == kind).count();
    assert_eq!(count(NeutralKind::Mineral), 1);
    assert_eq!(count(NeutralKind::StaticBuilding), 0);

    assert!(layout.is_reserved(TilePosition::new(2, 2), 1, 1));
    assert_eq!(layout.used_by(TilePosition::new(2, 2), 1, 1), None);
    assert!(!layout.is_reserved(TilePosition::new(5, 5), 1, 1));
    assert_eq!(layout.used_by(TilePosition::new(5, 5), 1, 1), None);
    assert!(layout.is_reserved(TilePosition::new(11, 2), 1, 1));
    assert_eq!(
        layout.used_by(TilePosition::new(11, 2), 1, 1),
        Some(UnitType::RESOURCE_MINERAL_FIELD.id())
    );
}

#[test]
fn reserves_and_used_tiles_are_flags() {
    let (_, mut layout) = layout(&terrain(&rows(&open(10, 10))));
    let t = TilePosition::new(3, 3);

    layout.add_reserve(t, 2, 2);
    assert!(layout.is_reserved(TilePosition::new(4, 4), 1, 1));
    assert!(layout.is_reserved(TilePosition::new(0, 0), 4, 4));
    // A reserve is a flag: removing one frees what overlaps it.
    layout.add_reserve(TilePosition::new(4, 4), 2, 2);
    layout.remove_reserve(t, 2, 2);
    assert!(!layout.is_reserved(TilePosition::new(4, 4), 1, 1));
    assert!(layout.is_reserved(TilePosition::new(5, 5), 1, 1));
    // Tiles outside the map are left alone.
    layout.add_reserve(TilePosition::new(8, 8), 4, 4);
    assert!(layout.is_reserved(TilePosition::new(9, 9), 3, 3));

    layout.add_building(&pylon(), t);
    assert_eq!(layout.used_by(t, 1, 1), Some(UnitType::PROTOSS_PYLON.id()));
    layout.remove_building(&pylon(), t);
    assert_eq!(layout.used_by(t, 2, 2), None);
}

#[test]
fn a_building_needs_buildable_walkable_unused_tiles() {
    let mut lines = open(20, 20);
    lines[5].replace_range(5..6, ",");
    lines[10].replace_range(10..11, "#");
    let (bwem, mut layout) = layout(&terrain(&rows(&lines)));
    let placeable = |layout: &bweb::Layout, fp: &Footprint, x, y| {
        layout.can_place(&bwem, fp, TilePosition::new(x, y))
    };

    assert!(placeable(&layout, &pylon(), 1, 1));
    assert!(!placeable(&layout, &pylon(), 4, 4), "unbuildable tile");
    assert!(!placeable(&layout, &pylon(), 9, 9), "unwalkable tile");
    assert!(!placeable(&layout, &pylon(), 19, 1), "outside the map");
    assert!(!placeable(&layout, &pylon(), -1, 1), "outside the map");

    layout.add_building(&pylon(), TilePosition::new(14, 14));
    assert!(!placeable(&layout, &pylon(), 13, 13), "used tile");
    assert!(placeable(&layout, &pylon(), 16, 16));

    // A building on creep needs the row below it buildable.
    assert!(placeable(&layout, &spawning_pool(), 1, 1));
    assert!(!placeable(&layout, &spawning_pool(), 1, 18));
    assert!(
        !placeable(&layout, &spawning_pool(), 4, 3),
        "unbuildable row below"
    );
}

#[test]
fn a_resource_depot_keeps_away_from_resources() {
    let mut input = terrain(&rows(&open(40, 40)));
    input.neutrals.push(mineral(1, 20, 20));
    input.neutrals.push(geyser(2, 20, 32));
    let (bwem, layout) = layout(&input);
    let cc = command_center();
    let placeable = |x, y| layout.can_place(&bwem, &cc, TilePosition::new(x, y));

    // A mineral at (20, 20) bars the tops left x in 14..25 and y in 15..24.
    assert!(!placeable(14, 16));
    assert!(placeable(13, 16));
    assert!(!placeable(24, 16) && placeable(25, 16));
    assert!(!placeable(16, 15) && placeable(16, 14));
    assert!(!placeable(16, 23) && placeable(16, 24));
    // A geyser at (20, 32) bars x in 14..27 and y in 27..37.
    assert!(!placeable(26, 28) && placeable(27, 28));
    assert!(!placeable(14, 28) && placeable(13, 28));
    assert!(!placeable(16, 27) && placeable(16, 26));
    // Other buildings do not care.
    assert!(layout.can_place(&bwem, &pylon(), TilePosition::new(17, 17)));
}

/// Two rooms of 24×24 tiles, one above the other, joined by a gap of 2 tiles,
/// each with a mineral line and a geyser.
fn two_bases() -> MapInput {
    let mut lines = open(24, 24);
    let wall = format!("{}..{}", "#".repeat(11), "#".repeat(11));
    lines.push(wall.clone());
    lines.push(wall);
    lines.extend(open(24, 24));
    let mut input = terrain(&rows(&lines));
    for i in 0..8 {
        input
            .neutrals
            .push(mineral(100 + i, 4 + 2 * (i % 4), 1 + i / 4));
        input
            .neutrals
            .push(mineral(200 + i, 4 + 2 * (i % 4), 47 + i / 4));
    }
    input.neutrals.push(geyser(300, 16, 2));
    input.neutrals.push(geyser(301, 16, 46));
    input
}

#[test]
fn ground_distance_goes_through_choke_points() {
    let (bwem, layout) = layout(&two_bases());
    let (top, bottom) = (Position::new(384, 400), Position::new(384, 1200));

    let through = layout
        .ground_distance_via_choke_points(&bwem, top, bottom)
        .unwrap();
    assert!(through >= top.distance(bottom));
    // In one area, the straight line.
    let near = Position::new(500, 400);
    assert_eq!(
        layout.ground_distance_via_choke_points(&bwem, top, near),
        Some(top.distance(near))
    );
    // Outside the map.
    assert_eq!(
        layout.ground_distance_via_choke_points(&bwem, Position::new(-1, -1), top),
        None
    );
    assert_eq!(
        layout.ground_distance_via_choke_points(&bwem, top, Position::new(768, 0)),
        None
    );

    // The closest tile of the choke point is in the gap.
    let cp = bwem.areas().next().unwrap().choke_points().next().unwrap();
    let tile = layout.closest_choke_point_tile(cp, top).unwrap();
    assert!((11 * 32..13 * 32).contains(&tile.x), "{tile:?}");
}

#[test]
fn rooms_without_a_way_between_have_no_ground_distance() {
    let mut lines = open(24, 24);
    lines.push("#".repeat(24));
    lines.push("#".repeat(24));
    lines.extend(open(24, 24));
    let (bwem, layout) = layout(&terrain(&rows(&lines)));
    assert_eq!(
        layout.ground_distance_via_choke_points(
            &bwem,
            Position::new(384, 300),
            Position::new(384, 1300)
        ),
        None
    );
}

#[test]
fn perpendicular_line_crosses_the_middle() {
    let (a, b) = perpendicular_line((Position::new(0, 0), Position::new(10, 0)), 5.0);
    assert_eq!((a, b), (Position::new(5, 5), Position::new(5, -5)));
}

/// `getClosestStation` and its kinds: the first station `keep` lets through
/// whose base center is nearest to `here`, by a strict `<` as in BWEB.
fn closest_station(
    stations: &Stations,
    here: Position,
    keep: impl Fn(&bweb::Station) -> bool,
) -> Option<&bweb::Station> {
    let mut best_distance = f64::MAX;
    let mut best = None;
    for station in stations {
        if !keep(station) {
            continue;
        }
        let distance = here.distance(station.center());
        if distance < best_distance {
            best_distance = distance;
            best = Some(station);
        }
    }
    best
}

#[test]
fn a_start_location_gets_a_main_and_the_next_base_is_its_natural() {
    let mut input = two_bases();
    let (bwem, _) = layout(&input);
    let locations: Vec<TilePosition> = bwem
        .areas()
        .flat_map(|a| a.bases())
        .map(|b| b.location())
        .collect();
    assert_eq!(locations.len(), 2);
    let top = *locations.iter().min_by_key(|t| t.y).unwrap();
    let bottom = *locations.iter().max_by_key(|t| t.y).unwrap();

    input.start_locations.push(top);
    let (bwem, mut layout) = layout(&input);
    let stations = Stations::analyze(&bwem, &mut layout, top, Race::TERRAN);

    assert_eq!(stations.iter().len(), 2);
    let main = stations.own_main().unwrap();
    let natural = stations.own_natural().unwrap();
    assert!(main.is_main() && !main.is_natural());
    assert!(natural.is_natural() && !natural.is_main());
    assert_eq!(bwem.base(main.base()).location(), top);
    assert_eq!(bwem.base(natural.base()).location(), bottom);
    assert_eq!(stations.station(main.base()), main);
    assert_eq!(stations.station(natural.base()), natural);
    let bottom_center = Position::from(bottom);
    assert_eq!(
        closest_station(&stations, bottom_center, |_| true),
        Some(natural)
    );
    assert_eq!(
        closest_station(&stations, bottom_center, bweb::Station::is_main),
        Some(main)
    );

    // Both hold the only choke point.
    let cp = main.choke_point().unwrap();
    assert_eq!(natural.choke_point(), Some(cp));

    // The resources are reserved, and the base location instead of used.
    let base = bwem.base(main.base());
    assert!(layout.is_reserved(top, 4, 3));
    assert_eq!(layout.used_by(top, 4, 3), None);
    for m in base.minerals() {
        assert!(layout.is_reserved(m.top_left(), 2, 1));
    }
    let count = base.minerals().count() as i32 + base.geysers().count() as i32;
    let sum = base
        .minerals()
        .map(|m| m.position())
        .chain(base.geysers().map(|g| g.position()))
        .fold(Position::new(0, 0), |a, b| a + b);
    assert_eq!(main.resource_centroid(), sum / count);

    // Terran presets: the slot of the ComSat right of the depot, reserved and
    // not used; no sim city.
    let comsat = top + TilePosition::new(4, 1);
    assert_eq!(main.small_slot(), Some(comsat));
    assert_eq!(main.medium_slot(), None);
    assert!(layout.is_reserved(comsat, 2, 2));
    assert_eq!(layout.used_by(comsat, 2, 2), None);
    for &d in main.defenses() {
        assert!(layout.is_reserved(d, 2, 2));
        assert_eq!(layout.used_by(d, 2, 2), None);
    }
}

/// What a station holds, in the order of the stations.
type StationFields = (
    bwem::BaseId,
    Position,
    Option<bwem::ChokePointId>,
    bool,
    bool,
    Position,
    u64,
    Vec<TilePosition>,
    Option<TilePosition>,
    Option<TilePosition>,
);

fn station_fields(stations: &Stations) -> Vec<StationFields> {
    stations
        .iter()
        .map(|s| {
            (
                s.base(),
                s.center(),
                s.choke_point(),
                s.is_main(),
                s.is_natural(),
                s.resource_centroid(),
                s.defense_angle().to_bits(),
                s.defenses().to_vec(),
                s.small_slot(),
                s.medium_slot(),
            )
        })
        .collect()
}

/// The reserves are flags, and the presets do not see them: analysing the
/// stations again on the layout they reserved finds the same stations and
/// changes nothing.
#[test]
fn analyzing_the_stations_twice_changes_nothing() {
    let mut input = two_bases();
    let (bwem, _) = layout(&input);
    let top = bwem
        .areas()
        .flat_map(|a| a.bases())
        .map(|b| b.location())
        .min_by_key(|t| t.y)
        .unwrap();
    input.start_locations.push(top);
    let (bwem, mut layout) = layout(&input);
    let size = layout.size();
    let snapshot = |layout: &bweb::Layout| -> Vec<(bool, Option<i32>)> {
        (0..size.y)
            .flat_map(|y| (0..size.x).map(move |x| TilePosition::new(x, y)))
            .map(|t| (layout.is_reserved(t, 1, 1), layout.used_by(t, 1, 1)))
            .collect()
    };

    for race in [Race::TERRAN, Race::ZERG, Race::PROTOSS] {
        let first = Stations::analyze(&bwem, &mut layout, top, race);
        let once = snapshot(&layout);
        let second = Stations::analyze(&bwem, &mut layout, top, race);
        assert_eq!(snapshot(&layout), once, "{race:?}");
        assert_eq!(station_fields(&second), station_fields(&first), "{race:?}");
    }
}

/// A room of 20×20 tiles with a wall at x = 10 from y = 0 to 15.
fn walled_room() -> MapInput {
    let mut lines = open(20, 20);
    for line in &mut lines[0..16] {
        line.replace_range(10..11, "#");
    }
    terrain(&rows(&lines))
}

fn terrain_walkable(layout: &bweb::Layout) -> impl Fn(&Path, TilePosition) -> bool + '_ {
    move |path, t| path.is_terrain_walkable(layout, t)
}

#[test]
fn a_star_goes_around_a_wall_without_cutting_corners() {
    let (_, layout) = layout(&walled_room());
    let mut pathfinding = Pathfinding::new(&layout);
    let (source, target) = (TilePosition::new(5, 5), TilePosition::new(15, 5));
    let mut path = Path::new(source, target, Footprint::NONE);
    path.astar(&mut pathfinding, &layout, terrain_walkable(&layout));

    assert!(path.is_reachable());
    let tiles = path.tiles();
    assert_eq!((tiles[0], *tiles.last().unwrap()), (source, target));
    // Around the end of the wall at y = 15, through y >= 16.
    assert!(tiles.iter().all(|t| t.x != 10 || t.y >= 16), "{tiles:?}");
    for pair in tiles.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        assert_eq!(a.chebyshev(b), 1, "{a:?} {b:?}");
        if a.x != b.x && a.y != b.y {
            // Both tiles beside a diagonal step are walkable.
            assert!(layout.is_walkable(TilePosition::new(b.x, a.y), &Footprint::NONE));
            assert!(layout.is_walkable(TilePosition::new(a.x, b.y), &Footprint::NONE));
        }
    }
    // BWEB does not measure A*.
    assert_eq!(path.distance(), 0.0);

    let mut reversed = Path::new(source, target, Footprint::NONE);
    reversed.set_reverse(true);
    reversed.astar(&mut pathfinding, &layout, terrain_walkable(&layout));
    let mut back = reversed.tiles().to_vec();
    back.reverse();
    assert_eq!(back, tiles);
}

#[test]
fn jps_gives_the_turns_and_their_length() {
    let (_, layout) = layout(&walled_room());
    let mut pathfinding = Pathfinding::new(&layout);
    let (source, target) = (TilePosition::new(5, 5), TilePosition::new(15, 5));
    let mut path = Path::new(source, target, Footprint::NONE);
    path.jps(&mut pathfinding, &layout, terrain_walkable(&layout));

    assert!(path.is_reachable());
    let tiles = path.tiles();
    assert_eq!((tiles[0], *tiles.last().unwrap()), (source, target));
    // Fewer tiles than steps: only the turns.
    assert!(tiles.len() < 12, "{tiles:?}");
    let length: f64 = tiles
        .windows(2)
        .map(|p| Position::from(p[0]).distance(Position::from(p[1])))
        .sum();
    assert_eq!(path.distance(), length);
    // Each leg is straight or diagonal.
    for pair in tiles.windows(2) {
        let (dx, dy) = ((pair[1].x - pair[0].x).abs(), (pair[1].y - pair[0].y).abs());
        assert!(dx == 0 || dy == 0 || dx == dy, "{pair:?}");
    }

    // A straight way is found at once.
    let mut straight = Path::new(source, TilePosition::new(5, 15), Footprint::NONE);
    straight.jps(&mut pathfinding, &layout, terrain_walkable(&layout));
    assert_eq!(straight.tiles(), [source, TilePosition::new(5, 15)]);
    assert_eq!(straight.distance(), 320.0);
    // From a tile to itself.
    let mut here = Path::new(source, source, Footprint::NONE);
    here.jps(&mut pathfinding, &layout, terrain_walkable(&layout));
    assert!(here.is_reachable());
    assert_eq!(here.tiles(), [source]);
}

#[test]
fn unit_walkability_avoids_buildings_but_for_the_source() {
    let (_, mut layout) = layout(&terrain(&rows(&open(20, 20))));
    let source = TilePosition::new(2, 2);
    layout.add_building(&pylon(), source);
    layout.add_building(
        &footprint(UnitType::PROTOSS_GATEWAY, (4, 18), (0, 0)),
        TilePosition::new(8, 0),
    );
    let path = Path::new(source, TilePosition::new(15, 2), Footprint::NONE);

    assert!(path.is_unit_walkable(&layout, source));
    assert!(!path.is_unit_walkable(&layout, TilePosition::new(3, 3)));
    assert!(path.is_terrain_walkable(&layout, TilePosition::new(3, 3)));

    let mut pathfinding = Pathfinding::new(&layout);
    let mut path = path;
    path.jps(&mut pathfinding, &layout, |path, t| {
        path.is_unit_walkable(&layout, t)
    });
    assert!(path.is_reachable());
    // Around the building at x = 8..12, below y = 18.
    assert!(path.tiles().iter().any(|t| t.y >= 18), "{:?}", path.tiles());
}

#[test]
fn a_failed_search_is_cached_until_the_layout_changes() {
    let mut lines = open(20, 20);
    for line in &mut lines {
        line.replace_range(10..11, "#");
    }
    let (_, mut layout) = layout(&terrain(&rows(&lines)));
    let mut pathfinding = Pathfinding::new(&layout);
    let (source, target) = (TilePosition::new(5, 5), TilePosition::new(15, 5));

    let mut path = Path::new(source, target, Footprint::NONE);
    path.astar(&mut pathfinding, &layout, terrain_walkable(&layout));
    assert!(!path.is_reachable());

    // BWEB skips a search whose source and target both failed, whatever it
    // walks on: here every tile.
    let mut anywhere = Path::new(source, target, Footprint::NONE);
    anywhere.astar(&mut pathfinding, &layout, |_, _| true);
    assert!(!anywhere.is_reachable());
    anywhere.jps(&mut pathfinding, &layout, |_, _| true);
    assert!(!anywhere.is_reachable());

    // A building away from the way changes the layout: the next search
    // forgets what failed before.
    layout.add_building(&pylon(), TilePosition::new(0, 18));
    anywhere.astar(&mut pathfinding, &layout, |_, _| true);
    assert!(anywhere.is_reachable());
}

/// The events of the layout clear the cache of failed searches before the
/// next search, and so does finding blocks, as in BWEB; a reserve does not.
/// The events here change the occupancy of tiles that hold nothing: BWEB frees
/// them whatever uses them. Not the destruction of a real neutral.
#[test]
fn the_events_of_the_layout_and_finding_blocks_clear_the_cache() {
    const FAR: TilePosition = TilePosition::new(0, 18);
    let mut lines = open(20, 20);
    for line in &mut lines {
        line.replace_range(10..11, "#");
    }
    let input = terrain(&rows(&lines));
    let (source, target) = (TilePosition::new(5, 5), TilePosition::new(15, 5));
    type Change = fn(&bwem::Map, &mut bweb::Layout);
    let changes: [(&str, Change); 5] = [
        ("add_building", |_, layout| {
            layout.add_building(&pylon(), FAR)
        }),
        ("remove_building", |_, layout| {
            layout.remove_building(&pylon(), FAR)
        }),
        ("remove_neutral", |_, layout| {
            layout.remove_neutral(&pylon(), FAR)
        }),
        ("Blocks::find", |bwem, layout| {
            let stations = Stations::analyze(bwem, layout, FAR, Race::TERRAN);
            let size = layout.size();
            let state = Snapshot {
                creep: Grid::filled(size.x, size.y, false),
                bwem,
                layout: layout.clone(),
            };
            Blocks::find(bwem, layout, &stations, Race::TERRAN, &state);
        }),
        ("add_reserve", |_, layout| layout.add_reserve(FAR, 2, 2)),
    ];

    for (name, change) in changes {
        let (bwem, mut layout) = layout(&input);
        let mut pathfinding = Pathfinding::new(&layout);
        let mut path = Path::new(source, target, Footprint::NONE);
        path.astar(&mut pathfinding, &layout, terrain_walkable(&layout));
        assert!(!path.is_reachable(), "{name}");

        change(&bwem, &mut layout);
        let mut anywhere = Path::new(source, target, Footprint::NONE);
        anywhere.jps(&mut pathfinding, &layout, |_, _| true);
        assert_eq!(anywhere.is_reachable(), name != "add_reserve", "{name}");
    }
}

#[test]
fn paths_with_the_same_ends_are_equal() {
    let a = Path::new(
        TilePosition::new(1, 1),
        TilePosition::new(2, 2),
        Footprint::NONE,
    );
    let b = Path::new(Position::new(40, 40), Position::new(70, 70), pylon());
    assert_eq!(a, b);
    assert_ne!(a, Path::default());
    assert_eq!(Path::default().source(), TilePosition::new(1000, 1000));
}

/// Creep on the tiles of `creep`; a building fits where `Layout` places it.
struct Snapshot<'a> {
    creep: Grid<TilePosition, bool>,
    bwem: &'a bwem::Map,
    layout: bweb::Layout,
}

impl StartState for Snapshot<'_> {
    fn has_creep(&self, tile: TilePosition) -> bool {
        self.creep.get(tile).copied().unwrap_or(false)
    }

    fn can_build_here(&self, tile: TilePosition, footprint: &Footprint) -> bool {
        self.layout.can_place(self.bwem, footprint, tile)
    }
}

#[test]
fn zerg_blocks_take_creep_and_reserve_their_slots() {
    let mut input = two_bases();
    let (bwem, _) = layout(&input);
    let top = bwem
        .areas()
        .flat_map(|a| a.bases())
        .map(|b| b.location())
        .min_by_key(|t| t.y)
        .unwrap();
    input.start_locations.push(top);
    let (bwem, mut layout) = layout(&input);
    let stations = Stations::analyze(&bwem, &mut layout, top, Race::ZERG);

    // Creep in the top room only: start blocks go there, production blocks
    // need none.
    let size = layout.size();
    let creep = Grid::from_fn(size.x, size.y, |t: TilePosition| t.y < 24);
    let state = Snapshot {
        creep,
        bwem: &bwem,
        layout: layout.clone(),
    };
    let blocks = Blocks::find(&bwem, &mut layout, &stations, Race::ZERG, &state);

    assert!(blocks.iter().next().is_some());
    for block in &blocks {
        let pieces = [
            block.small_tiles(),
            block.medium_tiles(),
            block.large_tiles(),
            block.wide_tiles(),
        ];
        assert_eq!(
            pieces.iter().map(|p| p.len()).sum::<usize>(),
            1,
            "Zerg blocks are one piece"
        );
        for &t in pieces.iter().flat_map(|p| p.iter()) {
            assert!(layout.is_reserved(t, 1, 1), "{t:?}");
        }
        // The slots for a building go by its size.
        let fp = |ty| Footprint::of(ty).unwrap();
        assert_eq!(
            block.placements(&fp(UnitType::ZERG_SPIRE)),
            block.small_tiles()
        );
        assert_eq!(
            block.placements(&fp(UnitType::ZERG_SPAWNING_POOL)),
            block.medium_tiles()
        );
        assert_eq!(
            block.placements(&fp(UnitType::ZERG_HATCHERY)),
            block.large_tiles()
        );
    }
    // At most 1 small, 5 medium and 1 wide start block by the main.
    let count = |f: fn(&bweb::Block) -> usize| blocks.iter().map(f).sum::<usize>();
    assert!(count(|b| b.small_tiles().len()) <= 1);
    assert!(count(|b| b.medium_tiles().len()) <= 5);

    // The closest block, and removing the block that covers a tile.
    let mut blocks = blocks;
    let first = blocks.iter().next().unwrap().clone();
    let inside = first.tile();
    assert_eq!(
        blocks.closest(inside + TilePosition::new(first.width() / 2, first.height() / 2)),
        Some(first.id())
    );
    let before = blocks.iter().count();
    let covering = blocks.covering(inside).unwrap();
    assert!(blocks.remove(covering));
    assert_eq!(blocks.iter().count(), before - 1);
    assert!(!blocks.iter().any(|b| *b == first));
}

/// Whether each tile is reserved, row by row.
fn reserves(layout: &bweb::Layout) -> Vec<bool> {
    let size = layout.size();
    (0..size.y)
        .flat_map(|y| (0..size.x).map(move |x| TilePosition::new(x, y)))
        .map(|t| layout.is_reserved(t, 1, 1))
        .collect()
}

#[test]
fn a_wall_that_does_not_fit_reserves_nothing() {
    // The pieces of BWEB's walls go within 8 tiles of the natural's location;
    // here its choke point is 20 tiles away. Walls that fit are checked on real
    // maps against C++ BWEB (`openbw-host`).
    let mut input = two_bases();
    let (bwem, _) = layout(&input);
    let top = bwem
        .areas()
        .flat_map(|a| a.bases())
        .map(|b| b.location())
        .min_by_key(|t| t.y)
        .unwrap();
    input.start_locations.push(top);
    let (bwem, mut layout) = layout(&input);
    let size = layout.size();
    let natural = |stations: &Stations| stations.own_natural().and_then(|s| s.choke_point());

    for race in [Race::ZERG, Race::TERRAN, Race::PROTOSS] {
        let stations = Stations::analyze(&bwem, &mut layout, top, race);
        let state = Snapshot {
            creep: Grid::filled(size.x, size.y, true),
            bwem: &bwem,
            layout: layout.clone(),
        };
        let before = reserves(&layout);
        let mut walls = Walls::new(&bwem);
        let wall = WallSpec::natural(race, Race::ZERG, &bwem, &stations)
            .and_then(|spec| walls.build(&bwem, &mut layout, &stations, &state, &spec));
        assert_eq!(wall, Err(WallError::NothingFits), "{race:?}");
        assert_eq!(reserves(&layout), before, "{race:?}");
        assert_eq!(walls.iter().count(), 0);
        let choke = natural(&stations).expect("the natural holds the gap");
        assert!(walls.wall(choke).is_none());
        assert_eq!(
            walls.add_defense_layer(&bwem, &layout, &stations, &state, choke),
            Err(WallError::NoWall)
        );
    }
}
