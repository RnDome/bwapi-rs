//! BWEM on synthetic maps drawn tile by tile.

use bwapi::{Position, TilePosition, WalkPosition};
use bwapi_sdk::bwem::{InputError, Map, MapInput, NeutralInput, NeutralKind, Node};
use bwapi_sdk::grid::Grid;

/// One character per tile: `.` buildable ground, `,` walkable unbuildable
/// ground, `#` unwalkable, `H` buildable very high ground.
fn terrain(rows: &[&str]) -> MapInput {
    let (width, height) = (rows[0].len() as i32, rows.len() as i32);
    let at = |t: TilePosition| rows[t.y as usize].as_bytes()[t.x as usize];
    MapInput {
        walkable: Grid::from_fn(width * 4, height * 4, |w: WalkPosition| {
            at(TilePosition::from(w)) != b'#'
        }),
        buildable: Grid::from_fn(width, height, |t| matches!(at(t), b'.' | b'H')),
        ground_height: Grid::from_fn(width, height, |t| if at(t) == b'H' { 4 } else { 0 }),
        neutrals: Vec::new(),
        start_locations: Vec::new(),
    }
}

fn neutral(unit_id: i32, kind: NeutralKind, x: i32, y: i32, amount: i32) -> NeutralInput {
    let size = match kind {
        NeutralKind::Mineral => TilePosition::new(2, 1),
        NeutralKind::Geyser => TilePosition::new(4, 2),
        NeutralKind::StaticBuilding | NeutralKind::Other => TilePosition::new(2, 2),
    };
    let top_left = TilePosition::new(x, y);
    NeutralInput {
        unit_id,
        type_id: 0,
        kind,
        top_left,
        size,
        pos: Position::from(top_left) + Position::from(size) / 2,
        initial_amount: amount,
        tile_position: top_left,
        building: kind != NeutralKind::Other,
        immobile: true,
    }
}

fn mineral(unit_id: i32, x: i32, y: i32, amount: i32) -> NeutralInput {
    neutral(unit_id, NeutralKind::Mineral, x, y, amount)
}

fn geyser(unit_id: i32, x: i32, y: i32, amount: i32) -> NeutralInput {
    neutral(unit_id, NeutralKind::Geyser, x, y, amount)
}

/// `width × height` tiles of ground.
fn open(width: usize, height: usize) -> Vec<String> {
    vec![".".repeat(width); height]
}

fn rows(lines: &[String]) -> Vec<&str> {
    lines.iter().map(String::as_str).collect()
}

/// Two rooms of 24×24 tiles, one above the other, with a wall between them and a
/// gap of `gap` tiles in the middle of the wall.
fn two_rooms(gap: usize) -> Vec<String> {
    let mut lines = open(24, 24);
    let wall = format!(
        "{}{}{}",
        "#".repeat(11),
        ".".repeat(gap),
        "#".repeat(13 - gap)
    );
    lines.push(wall.clone());
    lines.push(wall);
    lines.extend(open(24, 24));
    lines
}

#[test]
fn altitude_grows_from_the_edges() {
    let map = Map::initialize(&terrain(&rows(&open(16, 16)))).unwrap();

    let altitude = |x, y| map.mini_tile(WalkPosition::new(x, y)).unwrap().altitude();
    // The mini tiles around the map count as sea.
    assert_eq!(altitude(0, 0), 8);
    assert_eq!(altitude(1, 1), 16);
    assert_eq!(altitude(5, 0), 8);
    // 32 mini tiles from the edge, as in the middle of 64 mini tiles.
    assert_eq!(map.max_altitude(), 256);
    assert_eq!(altitude(31, 31), 256);
    assert_eq!(map.areas().len(), 1);
    assert_eq!(map.choke_point_count(), 0);
}

#[test]
fn a_small_unwalkable_blob_is_a_lake_and_a_wide_band_is_sea() {
    let mut lines = open(40, 40);
    for line in &mut lines[10..12] {
        line.replace_range(10..12, "##");
    }
    for line in &mut lines {
        line.replace_range(30..40, &"#".repeat(10));
    }
    let map = Map::initialize(&terrain(&rows(&lines))).unwrap();

    let mini = |x, y| map.mini_tile(WalkPosition::new(x, y)).unwrap();
    // The blob of 2×2 tiles is 8×8 mini tiles: an unwalkable mini tile makes its
    // neighbours unwalkable, but every mini tile of a buildable or fully walkable
    // tile is walkable again.
    assert!(mini(40, 40).is_lake());
    assert!(mini(47, 47).is_lake());
    assert!(!mini(39, 40).is_lake() && mini(39, 40).is_walkable());
    assert!(mini(140, 10).is_sea());
    // A lake has an altitude: areas flow around it.
    assert!(mini(44, 44).altitude() > 0);
    assert_eq!(map.areas().len(), 1);
}

#[test]
fn a_gap_between_two_rooms_is_a_choke_point() {
    let map = Map::initialize(&terrain(&rows(&two_rooms(2)))).unwrap();

    assert_eq!(map.areas().len(), 2);
    assert_eq!(map.choke_point_count(), 1);
    let top = map.area_at(TilePosition::new(12, 5)).unwrap();
    let bottom = map.area_at(TilePosition::new(12, 45)).unwrap();
    assert_ne!(top, bottom);
    assert!(top.is_reachable_from(bottom.id()));

    let cp = top.choke_points().next().unwrap();
    assert!(!cp.is_pseudo() && !cp.is_blocked());
    let (a, b) = cp.areas();
    assert!((a == top && b == bottom) || (a == bottom && b == top));
    assert_eq!(top.choke_points_to(bottom.id()).len(), 1);
    assert_eq!(bottom.choke_points_to(top.id()).len(), 1);
    assert_eq!(top.accessible_neighbours().collect::<Vec<_>>(), [bottom]);

    // The frontier lies in the gap: tiles 11..13 by x, around the wall by y.
    for &w in cp.geometry() {
        let t = TilePosition::from(w);
        assert!((11..13).contains(&t.x), "{w:?} outside the gap");
    }
    for node in [Node::End1, Node::Middle, Node::End2] {
        assert_eq!(
            map.mini_tile(cp.node_in_area(node, top.id()))
                .unwrap()
                .area_id(),
            top.id().get()
        );
        assert_eq!(
            map.mini_tile(cp.node_in_area(node, bottom.id()))
                .unwrap()
                .area_id(),
            bottom.id().get()
        );
    }
    assert!(cp.width() > 0);

    // From room to room through the choke point.
    let path = map
        .choke_point_path(Position::new(384, 300), Position::new(384, 1300))
        .unwrap();
    assert_eq!(path.len(), 1);
    assert!(path.first().unwrap().id() == cp.id());
    let length = path.length().unwrap();
    assert!((1000..1200).contains(&length), "{length}");
    // In one area: no choke point, the approximate distance.
    let path = map
        .choke_point_path(Position::new(100, 100), Position::new(300, 100))
        .unwrap();
    assert!(path.is_empty());
    assert_eq!(path.length(), Some(200));
}

#[test]
fn a_wall_without_a_gap_makes_two_unreachable_groups() {
    let mut lines = open(24, 24);
    lines.push("#".repeat(24));
    lines.push("#".repeat(24));
    lines.extend(open(24, 24));
    let map = Map::initialize(&terrain(&rows(&lines))).unwrap();

    assert_eq!(map.areas().len(), 2);
    assert_eq!(map.choke_point_count(), 0);
    let top = map.area_at(TilePosition::new(12, 5)).unwrap();
    let bottom = map.area_at(TilePosition::new(12, 45)).unwrap();
    assert!(!top.is_reachable_from(bottom.id()));
    assert!(
        map.choke_point_path(Position::new(384, 300), Position::new(384, 1300))
            .is_none()
    );
}

#[test]
fn a_mineral_with_a_way_around_does_not_block() {
    let mut input = terrain(&rows(&two_rooms(4)));
    input.neutrals.push(mineral(7, 11, 24, 8));
    let map = Map::initialize(&input).unwrap();

    // Above, beside and below the mineral is one door: no blocking.
    assert!(!map.neutral_of_unit(7).unwrap().is_blocking());
    assert_eq!(map.choke_point_count(), 1);
    assert!(
        !map.areas()
            .next()
            .unwrap()
            .choke_points()
            .next()
            .unwrap()
            .is_pseudo()
    );
}

#[test]
fn a_mineral_that_closes_the_gap_separates_the_rooms_until_destroyed() {
    let mut input = terrain(&rows(&two_rooms(2)));
    input.neutrals.push(mineral(7, 11, 24, 8));
    let mut map = Map::initialize(&input).unwrap();

    let blocking = map.neutral_of_unit(7).unwrap();
    assert!(blocking.is_blocking());
    let top = map.area_at(TilePosition::new(12, 5)).unwrap();
    let bottom = map.area_at(TilePosition::new(12, 45)).unwrap();
    let blocked: Vec<_> = blocking.blocked_areas().collect();
    assert!(blocked.contains(&top) && blocked.contains(&bottom));
    // The mini tiles of the mineral are in no area.
    let under = WalkPosition::from(TilePosition::new(11, 24));
    assert!(map.mini_tile(under).unwrap().area_id() < 0);

    assert_eq!(map.choke_point_count(), 1);
    let pseudo = top.choke_points().next().unwrap();
    assert!(pseudo.is_pseudo() && pseudo.is_blocked());
    assert_eq!(pseudo.blocking_neutral().unwrap().unit_id(), 7);
    assert!(!top.is_reachable_from(bottom.id()));
    assert!(top.accessible_neighbours().next().is_none());
    assert!(
        map.choke_point_path(Position::new(384, 300), Position::new(384, 1300))
            .is_none()
    );
    let pseudo = pseudo.id();

    map.enable_automatic_path_analysis();
    let changes = map.remove_neutral(7).unwrap();
    assert_eq!(map.neutral(changes.neutral).unit_id(), 7);
    assert_eq!(changes.tiles.top_left, TilePosition::new(11, 24));
    assert_eq!(changes.tiles.size, TilePosition::new(2, 1));
    assert_eq!(changes.unblocked, [pseudo]);
    assert!(changes.topology_changed);
    assert!(map.remove_neutral(7).is_none());
    assert!(map.neutral_of_unit(7).is_none());

    let pseudo = map.choke_point(pseudo);
    assert!(!pseudo.is_blocked());
    assert!(pseudo.blocking_neutral().is_none());
    // The mini tiles of the mineral joined the first area it separated.
    assert!(map.mini_tile(under).unwrap().area_id() > 0);
    let top = map.area_at(TilePosition::new(12, 5)).unwrap();
    let bottom = map.area_at(TilePosition::new(12, 45)).unwrap();
    assert!(top.is_reachable_from(bottom.id()));
    let path = map
        .choke_point_path(Position::new(384, 300), Position::new(384, 1300))
        .unwrap();
    assert_eq!(path.len(), 1);
}

#[test]
fn without_automatic_path_update_a_destroyed_blocker_keeps_its_choke_point_blocked() {
    let mut input = terrain(&rows(&two_rooms(2)));
    input.neutrals.push(mineral(7, 11, 24, 8));
    let mut map = Map::initialize(&input).unwrap();

    let changes = map.remove_neutral(7).unwrap();
    assert!(changes.unblocked.is_empty());
    assert!(!changes.topology_changed);
    let top = map.area_at(TilePosition::new(12, 5)).unwrap();
    let pseudo = top.choke_points().next().unwrap();
    assert!(pseudo.is_blocked());
    assert!(pseudo.blocking_neutral().is_none());
}

/// A room of 40×24 tiles with a mineral line along the top edge and a geyser.
fn resource_room(minerals: bool, geysers: bool) -> MapInput {
    let mut input = terrain(&rows(&open(40, 24)));
    if minerals {
        for i in 0..8 {
            input
                .neutrals
                .push(mineral(100 + i, 14 + 2 * (i % 4), 2 + i / 4, 1500));
        }
    }
    if geysers {
        input.neutrals.push(geyser(200, 26, 8, 5000));
    }
    input
}

#[test]
fn minerals_and_a_geyser_make_one_base() {
    let map = Map::initialize(&resource_room(true, true)).unwrap();

    assert_eq!(map.base_count(), 1);
    let base = map.areas().next().unwrap().bases().next().unwrap();
    assert_eq!(base.minerals().count(), 8);
    assert_eq!(base.geysers().count(), 1);
    assert_eq!(base.blocking_minerals().count(), 0);
    assert!(!base.is_starting());
    // No resource within 3 tiles of the depot.
    let location = base.location();
    for m in base.minerals() {
        let gap_x = (m.top_left().x - (location.x + 4)).max(location.x - (m.bottom_right().x + 1));
        let gap_y = (m.top_left().y - (location.y + 3)).max(location.y - (m.bottom_right().y + 1));
        assert!(
            gap_x >= 3 || gap_y >= 3,
            "mineral {:?} too close to {location:?}",
            m.top_left()
        );
    }
    assert_eq!(
        base.center(),
        Position::from(location) + Position::new(64, 48)
    );
}

#[test]
fn minerals_alone_make_a_base() {
    let map = Map::initialize(&resource_room(true, false)).unwrap();
    assert_eq!(map.base_count(), 1);
}

#[test]
fn a_geyser_alone_makes_a_base() {
    let map = Map::initialize(&resource_room(false, true)).unwrap();
    assert_eq!(map.base_count(), 1);
}

#[test]
fn small_resources_make_no_base() {
    let mut input = resource_room(false, false);
    input.neutrals.push(mineral(1, 14, 2, 39));
    input.neutrals.push(geyser(2, 26, 8, 299));
    let map = Map::initialize(&input).unwrap();
    assert_eq!(map.base_count(), 0);
}

#[test]
fn a_start_location_takes_the_base_near_it() {
    let mut input = resource_room(true, true);
    let first = Map::initialize(&input).unwrap();
    let location = first
        .areas()
        .next()
        .unwrap()
        .bases()
        .next()
        .unwrap()
        .location();

    let start = location + TilePosition::new(1, -1);
    input.start_locations.push(start);
    let mut map = Map::initialize(&input).unwrap();
    assert!(map.find_bases_for_starting_locations());
    let base = map.areas().next().unwrap().bases().next().unwrap();
    assert!(base.is_starting());
    assert_eq!(base.location(), start);
}

#[test]
fn a_start_location_without_resources_has_no_base() {
    let mut input = terrain(&rows(&open(24, 24)));
    input.start_locations.push(TilePosition::new(10, 10));
    let mut map = Map::initialize(&input).unwrap();
    assert_eq!(map.base_count(), 0);
    assert!(!map.find_bases_for_starting_locations());
}

#[test]
fn destroying_a_mineral_removes_it_from_its_area_and_base() {
    let mut map = Map::initialize(&resource_room(true, true)).unwrap();
    assert!(map.remove_neutral(103).is_some());

    assert_eq!(
        map.neutrals()
            .filter(|n| n.kind() == NeutralKind::Mineral)
            .count(),
        7
    );
    let area = map.areas().next().unwrap();
    assert_eq!(area.minerals().count(), 7);
    assert_eq!(area.bases().next().unwrap().minerals().count(), 7);
    assert!(
        map.tile(TilePosition::new(20, 2))
            .unwrap()
            .neutral()
            .is_none()
    );
}

#[test]
fn stacked_neutrals_share_their_tiles() {
    let mut input = resource_room(false, false);
    input.neutrals.push(mineral(1, 4, 4, 1500));
    input.neutrals.push(mineral(2, 4, 4, 1500));
    let mut map = Map::initialize(&input).unwrap();

    let tile = map.tile(TilePosition::new(4, 4)).unwrap();
    assert_eq!(tile.stacked_neutrals(), 2);
    assert_eq!(tile.neutral().unwrap().unit_id(), 1);
    assert_eq!(tile.neutral().unwrap().next_stacked().unwrap().unit_id(), 2);
    assert_eq!(
        map.tile(TilePosition::new(5, 4))
            .unwrap()
            .stacked_neutrals(),
        2
    );

    assert!(map.remove_neutral(1).is_some());
    let tile = map.tile(TilePosition::new(5, 4)).unwrap();
    assert_eq!(tile.stacked_neutrals(), 1);
    assert_eq!(tile.neutral().unwrap().unit_id(), 2);
}

#[test]
fn breadth_first_search_finds_the_nearest_cell() {
    let mut lines = open(16, 16);
    lines[8].replace_range(0..16, &"#".repeat(16));
    let map = Map::initialize(&terrain(&rows(&lines))).unwrap();

    // The first unwalkable tile in breadth first order: 5 rings away.
    let unwalkable = map
        .breadth_first_search(
            TilePosition::new(3, 3),
            |tile, _| !tile.is_walkable(),
            |_, _| true,
            true,
        )
        .unwrap();
    assert_eq!(unwalkable.y, 8);
    assert_eq!(unwalkable.chebyshev(TilePosition::new(3, 3)), 5);
    // The search does not cross cells it may not visit.
    let blocked = map.breadth_first_search(
        TilePosition::new(3, 3),
        |_, t| t.y > 8,
        |tile, _| tile.is_walkable(),
        true,
    );
    assert_eq!(blocked, None);
    // From the wall, the nearest area.
    assert!(map.nearest_area(TilePosition::new(3, 8)).is_some());
    assert!(map.area_at(TilePosition::new(3, 8)).is_none());
}

#[test]
fn analysis_is_deterministic() {
    let mut input = terrain(&rows(&two_rooms(3)));
    input.neutrals.push(mineral(1, 4, 4, 1500));
    input.neutrals.push(geyser(2, 10, 30, 5000));
    let summary = |map: &Map| {
        let areas: Vec<i16> = map.mini_tiles().map(|m| m.area_id()).collect();
        let cps: Vec<Vec<WalkPosition>> = map
            .areas()
            .flat_map(|a| a.choke_points().map(|cp| cp.geometry().to_vec()))
            .collect();
        let bases: Vec<TilePosition> = map
            .areas()
            .flat_map(|a| a.bases().map(|b| b.location()))
            .collect();
        (areas, cps, bases, map.raw_frontier().to_vec())
    };
    let first = Map::initialize(&input).unwrap();
    let second = Map::initialize(&input).unwrap();
    assert_eq!(summary(&first), summary(&second));
}

#[test]
fn inconsistent_input_is_an_error() {
    let mut input = terrain(&rows(&open(8, 8)));
    input.walkable = Grid::filled(31, 32, true);
    assert!(matches!(
        Map::initialize(&input),
        Err(InputError::GridSize {
            grid: "walkable",
            ..
        })
    ));

    let mut input = terrain(&rows(&open(8, 8)));
    input.neutrals.push(mineral(9, 7, 0, 100));
    assert_eq!(
        Map::initialize(&input).unwrap_err(),
        InputError::NeutralOutside { unit_id: 9 }
    );

    let mut input = terrain(&rows(&open(8, 8)));
    input.ground_height = Grid::filled(8, 8, 6);
    assert!(matches!(
        Map::initialize(&input),
        Err(InputError::GroundHeight { .. })
    ));

    let input = terrain(&rows(&open(8, 8)));
    let empty = MapInput {
        walkable: Grid::filled(0, 0, true),
        buildable: Grid::filled(0, 0, true),
        ground_height: Grid::filled(0, 0, 0),
        ..input
    };
    assert_eq!(Map::initialize(&empty).unwrap_err(), InputError::EmptyMap);
}
