// Port of BWEB (https://github.com/Cmccrave/BWEB, v1.17, d57980c), Wall.cpp
// and Wall.h.
// Copyright (c) 2017 Christian McCrave. MIT License, see LICENSE-BWEB.

use alloc::collections::{BTreeMap, BTreeSet, btree_map};
use alloc::vec::Vec;
use core::fmt;

use bwapi::{Position, Race, TilePosition, UnitType};

use super::layout::angle;
use super::{Footprint, Layout, StartState, Station, Stations};
use crate::PositionExt;
use crate::bwem::{self, AreaId, BaseId, ChokePointId};
use crate::grid::Grid;
use crate::math::{self, PI};

const QUARTER_PI: f64 = PI / 4.0;
const HALF_PI: f64 = PI / 2.0;

/// The footprint of a type the walls name.
fn known(ty: UnitType) -> Footprint {
    Footprint::of(ty).unwrap_or(Footprint::NONE)
}

/// A wall of buildings across a choke point, with the defenses and the
/// openings around it.
#[derive(Clone, Debug)]
pub struct Wall {
    /// The race of the bot, whose wall it is.
    race: Race,
    area: AreaId,
    choke_point: ChokePointId,
    station_base: BaseId,
    /// The location of the first base of the area: defenses line up from it.
    base_location: TilePosition,
    /// Where the pieces line up from: the base location, or toward the main's
    /// choke point when the natural's choke point can be walked around.
    wall_location: TilePosition,
    defense: Footprint,
    tight: Footprint,
    raw_buildings: Vec<Footprint>,
    raw_defenses: Vec<Footprint>,
    small_tiles: BTreeSet<TilePosition>,
    medium_tiles: BTreeSet<TilePosition>,
    large_tiles: BTreeSet<TilePosition>,
    openings: BTreeSet<TilePosition>,
    defenses: BTreeMap<i32, BTreeSet<TilePosition>>,
    /// The defenses of the station of the wall, which its own may take.
    station_defenses: BTreeSet<TilePosition>,
    defense_angle: f64,
    defense_arrangement: i32,
    pylon_wall: bool,
    open_wall: bool,
    require_tight: bool,
    flip_horizontal: bool,
    flip_vertical: bool,
}

static NO_TILES: BTreeSet<TilePosition> = BTreeSet::new();

impl Wall {
    /// The base of the station the wall is near.
    pub fn station_base(&self) -> BaseId {
        self.station_base
    }

    pub fn choke_point(&self) -> ChokePointId {
        self.choke_point
    }

    pub fn area(&self) -> AreaId {
        self.area
    }

    /// The defenses of layer `row`, 1 to 3, or of every layer, 0.
    pub fn defenses(&self, row: i32) -> &BTreeSet<TilePosition> {
        self.defenses.get(&row).unwrap_or(&NO_TILES)
    }

    /// The 1×1 tiles left open for units.
    pub fn openings(&self) -> &BTreeSet<TilePosition> {
        &self.openings
    }

    pub fn large_tiles(&self) -> &BTreeSet<TilePosition> {
        &self.large_tiles
    }

    pub fn medium_tiles(&self) -> &BTreeSet<TilePosition> {
        &self.medium_tiles
    }

    pub fn small_tiles(&self) -> &BTreeSet<TilePosition> {
        &self.small_tiles
    }

    /// The buildings the wall was asked for.
    pub fn raw_buildings(&self) -> &[Footprint] {
        &self.raw_buildings
    }

    /// The defenses the wall was asked for. BWEB keeps them only: the defenses
    /// are of the race.
    pub fn raw_defenses(&self) -> &[Footprint] {
        &self.raw_defenses
    }

    /// The unit the wall was asked to be tight against. BWEB keeps it only.
    pub fn tight_against(&self) -> Footprint {
        self.tight
    }

    /// Whether the wall leaves openings for units.
    pub fn is_open(&self) -> bool {
        self.open_wall
    }

    /// Whether the wall was asked to be tight. BWEB keeps it only.
    pub fn is_tight_required(&self) -> bool {
        self.require_tight
    }

    /// Whether the wall has more than one Pylon.
    pub fn is_pylon_wall(&self) -> bool {
        self.pylon_wall
    }

    /// The angle in radians the defenses line up along.
    pub fn defense_angle(&self) -> f64 {
        self.defense_angle
    }

    fn count_defenses(&self, layout: &Layout, types: [UnitType; 3]) -> i32 {
        let used = |tile| layout.used_by(tile, 1, 1);
        self.defenses(0)
            .iter()
            .filter(|&&tile| types.iter().any(|t| used(tile) == Some(t.id())))
            .count() as i32
    }

    /// The defenses of the wall that stand: Photon Cannons, Sunken Colonies
    /// and Bunkers on its defense tiles, as `layout` has them used.
    pub fn ground_defense_count(&self, layout: &Layout) -> i32 {
        self.count_defenses(
            layout,
            [
                UnitType::PROTOSS_PHOTON_CANNON,
                UnitType::ZERG_SUNKEN_COLONY,
                UnitType::TERRAN_BUNKER,
            ],
        )
    }

    /// The air defenses of the wall that stand: Photon Cannons, Spore
    /// Colonies and Missile Turrets on its defense tiles.
    pub fn air_defense_count(&self, layout: &Layout) -> i32 {
        self.count_defenses(
            layout,
            [
                UnitType::PROTOSS_PHOTON_CANNON,
                UnitType::ZERG_SPORE_COLONY,
                UnitType::TERRAN_MISSILE_TURRET,
            ],
        )
    }

    fn piece_count(&self) -> usize {
        self.small_tiles.len() + self.medium_tiles.len() + self.large_tiles.len()
    }
}

/// What wall to make, the arguments of `Walls::createWall`: of `race`, the
/// race of the bot, across `choke_point` from `area`. By default with no
/// buildings and no defenses, closed, and tight against nothing.
#[derive(Clone, Debug)]
pub struct WallSpec {
    race: Race,
    area: AreaId,
    choke_point: ChokePointId,
    buildings: Vec<Footprint>,
    defenses: Vec<Footprint>,
    tight: Footprint,
    open: bool,
    require_tight: bool,
}

impl WallSpec {
    pub fn across(race: Race, area: AreaId, choke_point: ChokePointId) -> WallSpec {
        WallSpec {
            race,
            area,
            choke_point,
            buildings: Vec::new(),
            defenses: Vec::new(),
            tight: Footprint::NONE,
            open: false,
            require_tight: false,
        }
    }

    /// The buildings of the wall, each a piece of its size.
    pub fn buildings(mut self, buildings: impl IntoIterator<Item = Footprint>) -> WallSpec {
        self.buildings = buildings.into_iter().collect();
        self
    }

    /// The defenses asked for. BWEB keeps them only: the wall takes the
    /// defenses of its race.
    pub fn defenses(mut self, defenses: impl IntoIterator<Item = Footprint>) -> WallSpec {
        self.defenses = defenses.into_iter().collect();
        self
    }

    /// The unit the wall should leave no gap for. BWEB keeps it only.
    pub fn tight_against(mut self, unit: Footprint) -> WallSpec {
        self.tight = unit;
        self
    }

    /// Leaves openings for units.
    pub fn open(mut self) -> WallSpec {
        self.open = true;
        self
    }

    /// Asks for a tight wall. BWEB keeps it only.
    pub fn require_tight(mut self) -> WallSpec {
        self.require_tight = true;
        self
    }

    /// The presets of BWEB, across the choke point of the bot's own natural:
    /// `createZergWall`, a Hatchery and an Evolution Chamber with Sunken
    /// Colonies, open; `createProtossWall`, a Forge, a Gateway and a Pylon
    /// with Photon Cannons, open; `createTerranWall`, for Terran and any other
    /// race, a Terran wall of a Barracks and two Supply Depots with Missile
    /// Turrets, closed and tight against a Zergling or, but for Zerg `enemy`, a
    /// Zealot.
    pub fn natural(
        race: Race,
        enemy: Race,
        bwem: &bwem::Map,
        stations: &Stations,
    ) -> Result<WallSpec, WallError> {
        let natural = stations.own_natural().ok_or(WallError::NoStation)?;
        let area = bwem.base(natural.base()).area().id();
        let choke_point = natural.choke_point().ok_or(WallError::NoChokePoint)?;
        let spec = |race| WallSpec::across(race, area, choke_point);
        let known_all = |types: &[UnitType]| types.iter().map(|&t| known(t)).collect::<Vec<_>>();
        Ok(match race {
            Race::ZERG => spec(Race::ZERG)
                .buildings(known_all(&[
                    UnitType::ZERG_HATCHERY,
                    UnitType::ZERG_EVOLUTION_CHAMBER,
                ]))
                .defenses([known(UnitType::ZERG_SUNKEN_COLONY)])
                .open(),
            Race::PROTOSS => spec(Race::PROTOSS)
                .buildings(known_all(&[
                    UnitType::PROTOSS_FORGE,
                    UnitType::PROTOSS_GATEWAY,
                    UnitType::PROTOSS_PYLON,
                ]))
                .defenses([known(UnitType::PROTOSS_PHOTON_CANNON)])
                .open(),
            _ => {
                let tight = match enemy {
                    Race::ZERG => UnitType::ZERG_ZERGLING,
                    _ => UnitType::PROTOSS_ZEALOT,
                };
                spec(Race::TERRAN)
                    .buildings(known_all(&[
                        UnitType::TERRAN_BARRACKS,
                        UnitType::TERRAN_SUPPLY_DEPOT,
                        UnitType::TERRAN_SUPPLY_DEPOT,
                    ]))
                    .defenses([known(UnitType::TERRAN_MISSILE_TURRET)])
                    .tight_against(Footprint {
                        type_id: tight.id(),
                        ..Footprint::NONE
                    })
                    .require_tight()
            }
        })
    }
}

/// Why there is no wall.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WallError {
    /// No station for the wall: the bot has no natural, or the area of the
    /// wall has no base or no station near it.
    NoStation,
    /// No choke point: the natural has none, or the main nearest the station
    /// of the wall has none.
    NoChokePoint,
    /// Not every building fits.
    NothingFits,
    /// No wall stands at the choke point.
    NoWall,
}

impl fmt::Display for WallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            WallError::NoStation => "no station for the wall",
            WallError::NoChokePoint => "no choke point for the wall",
            WallError::NothingFits => "not every building of the wall fits",
            WallError::NoWall => "no wall at the choke point",
        })
    }
}

impl core::error::Error for WallError {}

/// Every wall: `BWEB::Walls`.
#[derive(Clone, Debug)]
pub struct Walls {
    walls: BTreeMap<ChokePointId, Wall>,
    /// The tiles the wall being made takes, `wallReserveGrid`.
    reserved: Grid<TilePosition, bool>,
}

/// What a wall reads while it is made: `isPlaceable` of BWEB.
struct Ground<'a, 'm> {
    bwem: &'m bwem::Map,
    layout: &'a Layout,
    state: &'a dyn StartState,
}

impl Ground<'_, '_> {
    /// `Map::isPlaceable`: the rules of [`Layout::can_place`], and
    /// `canBuildHere` for a resource depot.
    fn placeable(&self, footprint: &Footprint, tile: TilePosition) -> bool {
        self.layout.can_place(self.bwem, footprint, tile)
            && (!footprint.is_resource_depot || self.state.can_build_here(tile, footprint))
    }
}

/// The tile offsets of the pieces of a wall, tried in order.
#[derive(Default)]
struct Orders {
    small: Vec<TilePosition>,
    medium: Vec<TilePosition>,
    large: Vec<TilePosition>,
    pylon: Vec<TilePosition>,
    /// Never cleared: each try of `addPieces` adds to it and flips it all
    /// again, as BWEB does.
    opening: Vec<TilePosition>,
}

fn offsets(list: &[(i32, i32)]) -> Vec<TilePosition> {
    list.iter().map(|&(x, y)| TilePosition::new(x, y)).collect()
}

impl Walls {
    pub fn new(bwem: &bwem::Map) -> Walls {
        let size = bwem.size();
        Walls {
            walls: BTreeMap::new(),
            reserved: Grid::filled(size.x, size.y, false),
        }
    }

    /// The walls by their choke points.
    pub fn iter(&self) -> btree_map::Values<'_, ChokePointId, Wall> {
        self.walls.values()
    }

    pub fn wall(&self, choke_point: ChokePointId) -> Option<&Wall> {
        self.walls.get(&choke_point)
    }

    /// `Walls::createWall`: the wall of `spec` with the defenses of its race
    /// and, for an open wall, openings; its choke point. If a wall stands at
    /// the choke point already, that one, as in BWEB. A wall found whole
    /// reserves its tiles in `layout`. Where BWEB dereferences a missing base,
    /// station or choke point, there is no wall.
    pub fn build(
        &mut self,
        bwem: &bwem::Map,
        layout: &mut Layout,
        stations: &Stations,
        state: &dyn StartState,
        spec: &WallSpec,
    ) -> Result<ChokePointId, WallError> {
        let choke_point = spec.choke_point;
        if self.walls.contains_key(&choke_point) {
            return Ok(choke_point);
        }
        let ground = Ground {
            bwem,
            layout,
            state,
        };
        let mut wall = Wall {
            race: spec.race,
            area: spec.area,
            choke_point,
            // Set by `initialize`.
            station_base: BaseId(0),
            base_location: TilePosition::new(0, 0),
            wall_location: TilePosition::new(0, 0),
            defense: Footprint::NONE,
            tight: spec.tight,
            raw_buildings: spec.buildings.clone(),
            raw_defenses: spec.defenses.clone(),
            small_tiles: BTreeSet::new(),
            medium_tiles: BTreeSet::new(),
            large_tiles: BTreeSet::new(),
            openings: BTreeSet::new(),
            defenses: BTreeMap::new(),
            station_defenses: BTreeSet::new(),
            defense_angle: 0.0,
            defense_arrangement: 0,
            pylon_wall: false,
            open_wall: spec.open,
            require_tight: spec.require_tight,
            flip_horizontal: false,
            flip_vertical: false,
        };
        let mut maker = Maker {
            ground: &ground,
            stations,
            race: spec.race,
            reserved: &mut self.reserved,
            orders: Orders::default(),
        };
        let made = maker
            .initialize(&mut wall)
            .and_then(|()| maker.add_pieces(&mut wall))
            .map(|()| {
                maker.add_defenses(&mut wall);
                maker.add_openings(&mut wall);
            });
        maker.cleanup(&wall);
        made?;
        if wall.piece_count() != wall.raw_buildings.len() {
            return Err(WallError::NothingFits);
        }

        for &tile in &wall.small_tiles {
            layout.add_reserve(tile, 2, 2);
        }
        for &tile in &wall.medium_tiles {
            layout.add_reserve(tile, 3, 2);
        }
        for &tile in &wall.large_tiles {
            layout.add_reserve(tile, 4, 3);
        }
        for &tile in &wall.openings {
            layout.add_reserve(tile, 1, 1);
        }
        for &tile in wall.defenses(0) {
            layout.add_reserve(tile, 2, 2);
        }
        self.walls.insert(choke_point, wall);
        Ok(choke_point)
    }

    /// `requestAddedLayer`: the pieces of the wall at `choke_point` given up
    /// for a layer of defenses 4 tiles out.
    pub fn add_defense_layer(
        &mut self,
        bwem: &bwem::Map,
        layout: &Layout,
        stations: &Stations,
        state: &dyn StartState,
        choke_point: ChokePointId,
    ) -> Result<(), WallError> {
        let wall = self.walls.get_mut(&choke_point).ok_or(WallError::NoWall)?;
        let ground = Ground {
            bwem,
            layout,
            state,
        };
        let mut maker = Maker {
            ground: &ground,
            stations,
            race: wall.race,
            reserved: &mut self.reserved,
            orders: Orders::default(),
        };
        for &tile in &wall.small_tiles {
            maker.unreserve(tile, 2, 2);
        }
        for &tile in &wall.medium_tiles {
            maker.unreserve(tile, 3, 2);
        }
        for &tile in &wall.large_tiles {
            maker.unreserve(tile, 4, 3);
        }
        wall.small_tiles.clear();
        wall.medium_tiles.clear();
        wall.large_tiles.clear();
        maker.add_defense_layer(wall, -4, 1, 6);
        Ok(())
    }
}

impl<'a> IntoIterator for &'a Walls {
    type Item = &'a Wall;
    type IntoIter = btree_map::Values<'a, ChokePointId, Wall>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// What making a wall needs besides the wall.
struct Maker<'a, 'g, 'm> {
    ground: &'a Ground<'g, 'm>,
    stations: &'a Stations,
    race: Race,
    reserved: &'a mut Grid<TilePosition, bool>,
    orders: Orders,
}

impl Maker<'_, '_, '_> {
    fn reserve(&mut self, t: TilePosition, width: i32, height: i32) {
        self.fill(t, width, height, true);
    }

    fn unreserve(&mut self, t: TilePosition, width: i32, height: i32) {
        self.fill(t, width, height, false);
    }

    fn fill(&mut self, t: TilePosition, width: i32, height: i32, value: bool) {
        for x in t.x..t.x + width {
            for y in t.y..t.y + height {
                if let Some(cell) = self.reserved.get_mut(TilePosition::new(x, y)) {
                    *cell = value;
                }
            }
        }
    }

    fn is_reserved(&self, here: TilePosition, footprint: &Footprint) -> bool {
        (here.x..here.x + footprint.tile_width).any(|x| {
            (here.y..here.y + footprint.tile_height)
                .any(|y| self.reserved.get(TilePosition::new(x, y)) == Some(&true))
        })
    }

    /// `Wall::initialize`: the station of the area, the defense angle and
    /// arrangement, and for a natural whose choke point can be walked around
    /// to the main, the wall moved toward the main's choke point.
    fn initialize(&mut self, wall: &mut Wall) -> Result<(), WallError> {
        let bwem = self.ground.bwem;
        let (area, choke_point) = (bwem.area(wall.area), bwem.choke_point(wall.choke_point));
        wall.pylon_wall = wall
            .raw_buildings
            .iter()
            .filter(|b| b.type_id == UnitType::PROTOSS_PYLON.id())
            .count()
            > 1;
        let base = area.bases().next().ok_or(WallError::NoStation)?;
        let station = self
            .stations
            .closest(Position::from(TilePosition::from(area.top())), |_| true)
            .ok_or(WallError::NoStation)?;
        let station_base = bwem.base(station.base());
        wall.station_base = station_base.id();
        wall.station_defenses = station.defenses().iter().copied().collect();
        wall.base_location = base.location();
        wall.wall_location = base.location();
        wall.defense_angle = station.defense_angle();
        wall.defense = known(match self.race {
            Race::PROTOSS => UnitType::PROTOSS_PHOTON_CANNON,
            Race::TERRAN => UnitType::TERRAN_MISSILE_TURRET,
            Race::ZERG => UnitType::ZERG_CREEP_COLONY,
            _ => UnitType::NONE,
        });

        // Stations without a choke point, or with several, get no angle on
        // start: the angle from the station to the last tile of the geometry.
        if !station.is_main() && !station.is_natural() {
            let mut best_geometry_tile = Position::from(choke_point.center()) + Position::new(4, 4);
            for &geometry_tile in choke_point.geometry() {
                let p = Position::from(geometry_tile) + Position::new(4, 4);
                if p.distance(station_base.center()) < f64::MAX {
                    best_geometry_tile = p;
                }
            }
            wall.defense_angle = angle(station_base.center(), best_geometry_tile) + HALF_PI;
        }
        wall.defense_arrangement = arrangement(wall.defense_angle);

        // A natural wall that can be walked around into the main walls at the
        // choke point instead.
        if station.is_natural() {
            let main = self.stations.closest(base.center(), Station::is_main);
            let main_choke = main
                .and_then(|m| m.choke_point())
                .map(|cp| bwem.choke_point(cp));
            if let Some(main_choke) = main_choke {
                let main_choke_center = Position::from(main_choke.center());
                let natural_choke_center = Position::from(choke_point.center());
                let to_main_choke = station_base.center().distance(main_choke_center);
                let to_natural_choke = station_base.center().distance(natural_choke_center);
                if to_natural_choke >= 240.0 && to_main_choke >= 240.0 {
                    let geometry = choke_point.geometry();
                    let bypass = geometry
                        .iter()
                        .filter(|&&geometry_tile| {
                            let p = Position::from(geometry_tile) + Position::new(4, 4);
                            p.distance(base.center()) >= p.distance(main_choke_center)
                        })
                        .count();
                    if bypass >= geometry.len() / 4 {
                        let main_choke_tile = TilePosition::from(main_choke_center);
                        let base_center_tile = TilePosition::from(base.center());
                        let toward = |d: i32| {
                            if d >= 2 {
                                d - 2
                            } else if d <= -2 {
                                d + 2
                            } else {
                                d
                            }
                        };
                        let dx = if wall.defense_arrangement == 0 {
                            toward(main_choke_tile.x - base_center_tile.x)
                        } else {
                            0
                        };
                        let dy = if wall.defense_arrangement == 2 {
                            toward(main_choke_tile.y - base_center_tile.y)
                        } else {
                            0
                        };
                        wall.wall_location =
                            base.location() + TilePosition::new(dx.clamp(-8, 8), dy.clamp(-8, 8));
                        let from = Position::from(wall.wall_location) + Position::new(16, 16);
                        wall.defense_angle = angle(from, natural_choke_center) + HALF_PI;
                        wall.defense_arrangement = arrangement(wall.defense_angle);
                    }
                }
            }
        }
        Ok(())
    }

    /// `Wall::tryLocations`: the first offset of `order` where a building of
    /// `footprint` fits, reserved and added to the pieces.
    fn try_locations(
        &mut self,
        wall: &mut Wall,
        order: &[TilePosition],
        footprint: &Footprint,
        piece: Piece,
    ) -> bool {
        let pylon = footprint.type_id == UnitType::PROTOSS_PYLON.id();
        let spire = footprint.type_id == UnitType::ZERG_SPIRE.id();

        // Defense types take any tile they fit, or a defense of the station.
        if footprint.tile_width == 2 && footprint.tile_height == 2 && !pylon && !spire {
            for &placement in order {
                let tile = wall.wall_location + placement;
                let station_defense = wall.station_defenses.contains(&tile);
                if (!self.is_reserved(tile, footprint) && self.ground.placeable(footprint, tile))
                    || station_defense
                {
                    wall.pieces_mut(piece).insert(tile);
                    self.reserve(tile, footprint.tile_width, footprint.tile_height);
                    return true;
                }
            }
            return false;
        }

        // Other types need one spot next to another piece, or for the first
        // piece next to unbuildable ground.
        let first = self.race == Race::PROTOSS || wall.piece_count() == 0;
        let (w, h) = (footprint.tile_width, footprint.tile_height);
        for &placement in order {
            let tile = wall.wall_location + placement;
            let mut valid = pylon;
            for x in -1..w + 1 {
                for y in -1..h + 1 {
                    let inside = x >= 0 && x < w && y >= 0 && y < h;
                    let corner = (x == -1 || x == w) && (y == -1 || y == h);
                    if inside || corner {
                        continue;
                    }
                    let border = tile + TilePosition::new(x, y);
                    if let Some(&reserved) = self.reserved.get(border) {
                        let unbuildable = !self.ground.layout.is_buildable(border);
                        if (first && unbuildable) || reserved {
                            valid = true;
                        }
                    }
                }
            }
            if !valid {
                continue;
            }
            if !self.is_reserved(tile, footprint)
                && self.ground.placeable(footprint, tile)
                && !self.ground.layout.is_reserved(tile, w, h)
            {
                wall.pieces_mut(piece).insert(tile);
                self.reserve(tile, w, h);
                return true;
            }
        }
        false
    }

    /// `Wall::addPieces`: the pieces at known offsets from the wall location by
    /// the angle of the choke point, moved farther out on each try, until all
    /// fit. An error where BWEB dereferences a missing choke point of the main.
    fn add_pieces(&mut self, wall: &mut Wall) -> Result<(), WallError> {
        let bwem = self.ground.bwem;
        let choke_point = bwem.choke_point(wall.choke_point);
        let base = bwem
            .area(wall.area)
            .bases()
            .next()
            .ok_or(WallError::NoStation)?;
        let station = self.stations.station(wall.station_base);
        let station_base = bwem.base(station.base());
        let closest_main = self
            .stations
            .closest(Position::from(station_base.location()), Station::is_main);
        let main_choke = closest_main
            .and_then(|m| m.choke_point())
            .map(|cp| bwem.choke_point(cp))
            .ok_or(WallError::NoChokePoint)?;
        let main_choke_center = Position::from(main_choke.center());
        let base_center = base.center();
        let choke_center = Position::from(choke_point.center());

        let (mut iteration, max_iteration) = if station.is_main() {
            (5, 8)
        } else if !station.is_natural() {
            (0, 4)
        } else {
            (0, 1)
        };

        while iteration < max_iteration {
            self.cleanup(wall);
            wall.small_tiles.clear();
            wall.medium_tiles.clear();
            wall.large_tiles.clear();

            let flip_order;
            let shift;
            match wall.defense_arrangement {
                // Horizontal.
                0 => {
                    wall.flip_vertical = base_center.y < choke_center.y;
                    self.orders.large = (-6..=6).map(|x| TilePosition::new(x, -6)).collect();
                    self.orders.medium = (-6..=6).map(|x| TilePosition::new(x, -5)).collect();
                    self.orders.small = (-2..=5).map(|x| TilePosition::new(x, -5)).collect();
                    self.orders.pylon = offsets(&[(0, -2), (2, -2)]);
                    flip_order = station.is_natural() && main_choke_center.x > base_center.x;
                    self.orders
                        .opening
                        .extend((-5..=8).map(|x| TilePosition::new(x, -4)));
                    shift = TilePosition::new(0, -iteration);
                }
                // Angled.
                1 | 3 => {
                    wall.flip_vertical = base_center.y < choke_center.y;
                    wall.flip_horizontal = base_center.x < choke_center.x;
                    self.orders.large = offsets(&[
                        (0, -7),
                        (0, -6),
                        (-2, -5),
                        (-2, -4),
                        (-4, -3),
                        (-4, -2),
                        (-6, -1),
                        (-6, 0),
                        (-8, 1),
                        (-8, 2),
                    ]);
                    self.orders.medium = offsets(&[
                        (1, -6),
                        (1, -5),
                        (-1, -4),
                        (-1, -3),
                        (-3, -2),
                        (-3, -1),
                        (-5, 0),
                        (-5, 1),
                    ]);
                    self.orders.small = offsets(&[(0, -4), (-2, -2), (-4, 0)]);
                    self.orders.pylon = offsets(&[(-2, 0), (0, -2)]);
                    let centroid = station.resource_centroid();
                    flip_order = (station.is_natural()
                        && main_choke_center.x < base_center.x
                        && main_choke_center.y > base_center.y)
                        || (station.is_natural()
                            && main_choke_center.x > base_center.x
                            && main_choke_center.y < base_center.y)
                        || (!station.is_main()
                            && !station.is_natural()
                            && centroid.x < station_base.center().x);
                    shift = TilePosition::new(-iteration, -iteration);
                }
                // Vertical.
                2 => {
                    wall.flip_horizontal = base_center.x < choke_center.x;
                    self.orders.large = (-4..=4).map(|y| TilePosition::new(-7, y)).collect();
                    self.orders.medium = (-3..=4).map(|y| TilePosition::new(-6, y)).collect();
                    self.orders.small = (-3..=4).map(|y| TilePosition::new(-5, y)).collect();
                    self.orders.pylon = offsets(&[(-2, 0), (-2, 2)]);
                    flip_order = station.is_natural() && main_choke_center.y > base_center.y;
                    shift = TilePosition::new(-iteration, 0);
                }
                _ => {
                    flip_order = false;
                    shift = TilePosition::new(0, 0);
                }
            }
            for order in [
                &mut self.orders.large,
                &mut self.orders.medium,
                &mut self.orders.small,
            ] {
                for tile in order.iter_mut() {
                    *tile = *tile + shift;
                }
            }
            // Away from the main and the natural, Zerg may take more large
            // pieces.
            if matches!(wall.defense_arrangement, 1 | 3)
                && !station.is_main()
                && !station.is_natural()
                && self.race == Race::ZERG
            {
                self.orders
                    .large
                    .extend(offsets(&[(-4, 0), (0, -3), (-4, -3)]));
            }

            // Flipped vertically and horizontally as needed, by the size of the
            // pieces: a Pylon, a Forge, a Gateway, a Dragoon.
            let flip = |order: &mut Vec<TilePosition>, (w, h): (i32, i32)| {
                if wall.flip_vertical {
                    for p in order.iter_mut() {
                        p.y = -(p.y - (3 - h));
                    }
                }
                if wall.flip_horizontal {
                    for p in order.iter_mut() {
                        p.x = -(p.x - (4 - w));
                    }
                }
                if flip_order {
                    order.reverse();
                }
            };
            flip(&mut self.orders.small, (2, 2));
            flip(&mut self.orders.pylon, (2, 2));
            flip(&mut self.orders.medium, (3, 2));
            flip(&mut self.orders.large, (4, 3));
            flip(&mut self.orders.opening, (1, 1));

            match self.race {
                Race::ZERG => {
                    for building in wall.raw_buildings.clone() {
                        let order = match building.tile_width {
                            4 => Some((
                                self.orders.large.clone(),
                                UnitType::ZERG_HATCHERY,
                                Piece::Large,
                            )),
                            3 => Some((
                                self.orders.medium.clone(),
                                UnitType::ZERG_EVOLUTION_CHAMBER,
                                Piece::Medium,
                            )),
                            2 => Some((
                                self.orders.small.clone(),
                                UnitType::ZERG_SPIRE,
                                Piece::Small,
                            )),
                            _ => None,
                        };
                        if let Some((order, ty, piece)) = order {
                            self.try_locations(wall, &order, &known(ty), piece);
                        }
                    }
                }
                Race::PROTOSS => {
                    // The Gateway across from the Forge, the opening between.
                    self.orders.large.reverse();
                    let (medium, large, pylon) = (
                        self.orders.medium.clone(),
                        self.orders.large.clone(),
                        self.orders.pylon.clone(),
                    );
                    self.try_locations(
                        wall,
                        &medium,
                        &known(UnitType::PROTOSS_FORGE),
                        Piece::Medium,
                    );
                    self.try_locations(
                        wall,
                        &large,
                        &known(UnitType::PROTOSS_GATEWAY),
                        Piece::Large,
                    );
                    self.try_locations(wall, &pylon, &known(UnitType::PROTOSS_PYLON), Piece::Small);
                }
                Race::TERRAN => {
                    for building in wall.raw_buildings.clone() {
                        let order = match building.tile_width {
                            4 => Some((
                                self.orders.large.clone(),
                                UnitType::TERRAN_BARRACKS,
                                Piece::Large,
                            )),
                            3 => Some((
                                self.orders.medium.clone(),
                                UnitType::TERRAN_SUPPLY_DEPOT,
                                Piece::Medium,
                            )),
                            _ => None,
                        };
                        if let Some((order, ty, piece)) = order {
                            self.try_locations(wall, &order, &known(ty), piece);
                        }
                    }
                }
                _ => {}
            }

            if wall.piece_count() == wall.raw_buildings.len() {
                break;
            }
            iteration += 2;
        }
        Ok(())
    }

    /// `Wall::addDefenseLayer`: defenses along the wall, `offset` tiles out,
    /// from the middle out to `width` each way, of layer `layer`. Those of
    /// later layers avoid reserved tiles but the station's defenses.
    fn add_defense_layer(&mut self, wall: &mut Wall, offset: i32, layer: i32, width: i32) {
        let (mut dx, mut dy, mut ox, mut oy, mut width) = (0, 0, 0, 0, width);
        match wall.defense_arrangement {
            0 => {
                dx = -1;
                oy = offset;
            }
            2 => {
                dy = -1;
                ox = offset;
            }
            1 | 3 => {
                dx = 1;
                dy = -1;
                oy = offset;
                width -= 2;
            }
            _ => {}
        }
        let at = |i: i32| TilePosition::new(dx * i + ox, dy * i + oy);
        let mut placements: Vec<TilePosition> = (0..=width).map(at).collect();
        placements.extend((-width..=0).rev().map(at));
        if wall.flip_vertical {
            for p in &mut placements {
                p.y = 1 - p.y;
            }
        }
        if wall.flip_horizontal {
            for p in &mut placements {
                p.x = 2 - p.x;
            }
        }

        let defense = wall.defense;
        for placement in placements {
            let tile = wall.base_location + placement;
            if self.ground.placeable(&defense, tile)
                && !self.is_reserved(tile, &defense)
                && (layer == 1
                    || !self.ground.layout.is_reserved(tile, 2, 2)
                    || wall.station_defenses.contains(&tile))
            {
                self.reserve(tile, defense.tile_width, defense.tile_height);
                wall.defenses.entry(0).or_default().insert(tile);
                wall.defenses.entry(layer).or_default().insert(tile);
            }
        }
    }

    /// `Wall::addDefenses`: three layers.
    fn add_defenses(&mut self, wall: &mut Wall) {
        self.add_defense_layer(wall, -2, 1, 3);
        self.add_defense_layer(wall, -2, 2, 6);
        self.add_defense_layer(wall, 0, 3, 6);
    }

    /// `Wall::addOpenings`: past the first tile of the wall, the free tiles of
    /// the opening line a Dragoon fits on.
    fn add_openings(&mut self, wall: &mut Wall) {
        let dragoon = known(UnitType::PROTOSS_DRAGOON);
        let mut found_wall = false;
        for placement in self.orders.opening.clone() {
            let tile = wall.wall_location + placement;
            if self.is_reserved(tile, &dragoon) {
                found_wall = true;
            }
            if found_wall
                && !self.is_reserved(tile, &dragoon)
                && self.ground.placeable(&dragoon, tile)
            {
                wall.openings.insert(tile);
                self.reserve(tile, 1, 1);
            }
        }
    }

    /// `Wall::cleanup`: the tiles of the wall no longer taken.
    fn cleanup(&mut self, wall: &Wall) {
        for &tile in &wall.small_tiles {
            self.unreserve(tile, 2, 2);
        }
        for &tile in &wall.medium_tiles {
            self.unreserve(tile, 3, 2);
        }
        for &tile in &wall.large_tiles {
            self.unreserve(tile, 4, 3);
        }
        for &tile in &wall.openings {
            self.unreserve(tile, 1, 1);
        }
        for tiles in wall.defenses.values() {
            for &tile in tiles {
                self.unreserve(tile, 2, 2);
            }
        }
    }
}

/// The kind of piece a slot goes to.
#[derive(Clone, Copy)]
enum Piece {
    Small,
    Medium,
    Large,
}

impl Wall {
    fn pieces_mut(&mut self, piece: Piece) -> &mut BTreeSet<TilePosition> {
        match piece {
            Piece::Small => &mut self.small_tiles,
            Piece::Medium => &mut self.medium_tiles,
            Piece::Large => &mut self.large_tiles,
        }
    }
}

/// `int(round(angle / M_PI_D4)) % 4`.
fn arrangement(angle: f64) -> i32 {
    (math::round(angle / QUARTER_PI) as i32) % 4
}
