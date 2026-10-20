// Port of BWEB (https://github.com/Cmccrave/BWEB, v1.17, d57980c), Station.cpp.
// Copyright (c) 2017 Christian McCrave. MIT License, see LICENSE-BWEB.

use alloc::collections::BTreeSet;
use alloc::vec;
use alloc::vec::Vec;

use bwapi::{Position, Race, TilePosition, UnitType};

use super::layout::angle;
use super::{Footprint, Layout};
use crate::PositionExt;
use crate::bwem::{self, Area, Base, BaseId, ChokePoint, ChokePointId, Node};
use crate::math::{self, PI};

const QUARTER_PI: f64 = PI / 4.0;
const HALF_PI: f64 = PI / 2.0;

/// A base with what BWEB finds around it: whether it is a main or a natural,
/// the choke point to hold, and the race presets: where defenses go, and the
/// slots of a Zerg sim city or of a Terran ComSat.
#[derive(Clone, Debug)]
pub struct Station {
    base: BaseId,
    center: Position,
    choke_point: Option<ChokePointId>,
    is_main: bool,
    is_natural: bool,
    resource_centroid: Position,
    defense_angle: f64,
    defenses: Vec<TilePosition>,
    small_slot: Option<TilePosition>,
    medium_slot: Option<TilePosition>,
}

impl Station {
    /// The base of the station: one station on every base.
    pub fn base(&self) -> BaseId {
        self.base
    }

    /// The center of the base.
    pub fn center(&self) -> Position {
        self.center
    }

    /// The choke point to hold: for a main or a natural, the way out of it;
    /// for another base, the only choke point of its area.
    pub fn choke_point(&self) -> Option<ChokePointId> {
        self.choke_point
    }

    pub fn is_main(&self) -> bool {
        self.is_main
    }

    pub fn is_natural(&self) -> bool {
        self.is_natural
    }

    /// The mean position of the minerals and geysers of the base.
    pub fn resource_centroid(&self) -> Position {
        self.resource_centroid
    }

    /// The angle in radians, from 0 counterclockwise on the screen, along
    /// which defenses line up: across the way from the base to its choke point
    /// as a rule. 0 without a choke point.
    pub fn defense_angle(&self) -> f64 {
        self.defense_angle
    }

    /// The top left tiles of the defenses of the race around the base: Photon
    /// Cannons, Missile Turrets or Creep Colonies; in increasing order.
    pub fn defenses(&self) -> &[TilePosition] {
        &self.defenses
    }

    /// The slot of the ComSat of a Terran base, or of the Spire of a Zerg
    /// main's sim city.
    pub fn small_slot(&self) -> Option<TilePosition> {
        self.small_slot
    }

    /// The slot of the Spawning Pool of a Zerg main's sim city.
    pub fn medium_slot(&self) -> Option<TilePosition> {
        self.medium_slot
    }
}

impl PartialEq for Station {
    fn eq(&self, other: &Self) -> bool {
        self.base == other.base
    }
}

impl Eq for Station {}

/// Every station of the map: `BWEB::Stations`.
#[derive(Clone, Debug)]
pub struct Stations {
    /// In the order of `findStations`: the mains, the naturals, then the other
    /// bases. Blocks follow it, and the nearest station of several at the same
    /// distance is the first.
    stations: Vec<Station>,
    /// The index in `stations` of the station of each base, by base.
    by_base: Vec<usize>,
    own_main: Option<BaseId>,
    own_natural: Option<BaseId>,
}

/// The state `findStations` keeps in globals while it creates the stations.
struct Finder<'a, 'm> {
    bwem: &'m bwem::Map,
    layout: &'a Layout,
    /// The layout as BWEB changes it while it creates the stations: each marks
    /// its tiles used, then frees them in `cleanup`.
    used: Layout,
    race: Race,
    /// The resource depot and the defense of the race, `UnitTypes::None` for
    /// another race.
    depot: Footprint,
    defense: Footprint,
    main_bases: Vec<Base<'m>>,
    natural_bases: Vec<Base<'m>>,
    main_choke_points: Vec<ChokePointId>,
    nonsense_choke_points: Vec<ChokePointId>,
}

/// The footprint of a type the presets name.
fn known(ty: UnitType) -> Footprint {
    Footprint::of(ty).unwrap_or(Footprint::NONE)
}

impl Stations {
    /// `Stations::findStations`: a main at every start location, a natural for
    /// every main, a station at every other base, with the presets of `race`,
    /// the race of the bot. `self_start` is its start location, whose main and
    /// natural are its own. BWEB marks the tiles of the presets used while it
    /// places them and frees them at the end of each station, which happens on
    /// a copy of `layout`; then `layout` gets the reserves of the stations.
    pub fn analyze(
        bwem: &bwem::Map,
        layout: &mut Layout,
        self_start: TilePosition,
        race: Race,
    ) -> Stations {
        let (depot, defense) = match race {
            Race::TERRAN => (
                UnitType::TERRAN_COMMAND_CENTER,
                UnitType::TERRAN_MISSILE_TURRET,
            ),
            Race::ZERG => (UnitType::ZERG_HATCHERY, UnitType::ZERG_CREEP_COLONY),
            Race::PROTOSS => (UnitType::PROTOSS_NEXUS, UnitType::PROTOSS_PHOTON_CANNON),
            _ => (UnitType::NONE, UnitType::NONE),
        };
        let mut finder = Finder {
            bwem,
            layout: &*layout,
            used: layout.clone(),
            race,
            depot: known(depot),
            defense: known(defense),
            main_bases: Vec::new(),
            natural_bases: Vec::new(),
            main_choke_points: Vec::new(),
            nonsense_choke_points: Vec::new(),
        };

        // Some choke points are nonsense: narrow, and next to another choke
        // point of their area. BWEB also loops over every area for nothing.
        for area in bwem.areas() {
            for cp in area.choke_points() {
                if cp.width() > 32 {
                    continue;
                }
                let next_to_another = area.choke_points().any(|other| {
                    other.id() != cp.id()
                        && other
                            .geometry()
                            .iter()
                            .any(|&geometry_tile| geometry_tile.distance(cp.center()) <= 4.0)
                });
                if next_to_another && !finder.nonsense_choke_points.contains(&cp.id()) {
                    finder.nonsense_choke_points.push(cp.id());
                }
            }
        }

        for area in bwem.areas() {
            finder
                .main_bases
                .extend(area.bases().filter(|b| b.is_starting()));
        }

        for i in 0..finder.main_bases.len() {
            let main = finder.main_bases[i];
            if let Some(natural) = finder.find_natural(main) {
                finder.natural_bases.push(natural);
            }
        }

        let mut stations = Vec::new();
        for i in 0..finder.main_bases.len() {
            stations.push(finder.station(finder.main_bases[i], true, false));
        }
        for i in 0..finder.natural_bases.len() {
            stations.push(finder.station(finder.natural_bases[i], false, true));
        }
        for area in bwem.areas() {
            for base in area.bases() {
                let listed = |bases: &[Base<'_>]| bases.iter().any(|b| b.id() == base.id());
                if listed(&finder.natural_bases) || listed(&finder.main_bases) {
                    continue;
                }
                stations.push(finder.station(base, false, false));
            }
        }

        let mut by_base = vec![usize::MAX; bwem.base_count()];
        for (i, station) in stations.iter().enumerate() {
            by_base[station.base.0 as usize] = i;
        }
        let mut result = Stations {
            stations,
            by_base,
            own_main: None,
            own_natural: None,
        };
        result.own_main = result
            .closest(Position::from(self_start), |s| s.is_main)
            .map(|s| s.base);
        if let Some(main) = result.own_main() {
            let here = match main.choke_point {
                Some(cp) => TilePosition::from(bwem.choke_point(cp).center()),
                None => TilePosition::from(main.center),
            };
            result.own_natural = result
                .closest(Position::from(here), |s| s.is_natural)
                .map(|s| s.base);
        }
        result.reserve(bwem, layout);
        result
    }

    /// The reserves of `Stations::findStations`, station by station: the tiles
    /// of each resource and a line of tiles from the depot to it
    /// (`addResourceReserves`), and the base location and the presets reserved
    /// instead of used (`cleanup`, which frees their tiles even of what used
    /// them before, as BWEB does).
    fn reserve(&self, bwem: &bwem::Map, layout: &mut Layout) {
        for station in &self.stations {
            let base = bwem.base(station.base);
            add_resource_reserves(bwem, layout, base);
            let mut free = |tile: TilePosition, width: i32, height: i32| {
                layout.remove_used(tile, width, height);
                layout.add_reserve(tile, width, height);
            };
            for &tile in &station.defenses {
                free(tile, 2, 2);
            }
            free(base.location(), 4, 3);
            if let Some(tile) = station.medium_slot {
                free(tile, 3, 2);
            }
            if let Some(tile) = station.small_slot {
                free(tile, 2, 2);
            }
        }
    }

    /// The stations in the order of `findStations`: the mains, the naturals,
    /// then the other bases.
    pub fn iter(&self) -> core::slice::Iter<'_, Station> {
        self.stations.iter()
    }

    /// The station of `base`.
    pub fn station(&self, base: BaseId) -> &Station {
        &self.stations[self.by_base[base.0 as usize]]
    }

    /// The main nearest to the start location of the bot: its own main. Not
    /// [`Station::is_main`], which holds for the main of every start location.
    pub fn own_main(&self) -> Option<&Station> {
        self.own_main.map(|base| self.station(base))
    }

    /// The natural nearest to the choke point of the bot's own main.
    pub fn own_natural(&self) -> Option<&Station> {
        self.own_natural.map(|base| self.station(base))
    }

    /// `getClosestStation` and its kinds: the first station `keep` lets
    /// through whose base center is nearest to `here`.
    pub(crate) fn closest(
        &self,
        here: Position,
        keep: impl Fn(&Station) -> bool,
    ) -> Option<&Station> {
        let mut best_distance = f64::MAX;
        let mut best = None;
        for station in &self.stations {
            if !keep(station) {
                continue;
            }
            let distance = here.distance(station.center);
            if distance < best_distance {
                best_distance = distance;
                best = Some(station);
            }
        }
        best
    }
}

impl<'a> IntoIterator for &'a Stations {
    type Item = &'a Station;
    type IntoIter = core::slice::Iter<'a, Station>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'m> Finder<'_, 'm> {
    /// `Map::getGroundDistance`, with `DBL_MAX` for no distance.
    fn ground_distance(&self, a: Position, b: Position) -> f64 {
        self.layout
            .ground_distance_via_choke_points(self.bwem, a, b)
            .unwrap_or(f64::MAX)
    }

    /// The natural of a main: the nearest base by ground with gas, at least 5
    /// minerals and a way out, within 1600 pixels; a base whose area has one
    /// choke point within 1600 pixels wins outright. Without one, the nearest
    /// other base.
    fn find_natural(&self, main: Base<'m>) -> Option<Base<'m>> {
        let mut base_best = None;
        let mut best_distance = f64::MAX;
        for area in self.bwem.areas() {
            for base in area.bases() {
                if base.is_starting()
                    || base.geysers().next().is_none()
                    || base.area().accessible_neighbours().next().is_none()
                    || base.minerals().count() < 5
                {
                    continue;
                }

                let distance = self.ground_distance(base.center(), main.center());

                // A pocket natural: its area has one choke point.
                if base.area().choke_points().len() == 1 && distance < 1600.0 {
                    best_distance = 0.0;
                    base_best = Some(base);
                }

                if distance < best_distance && distance < 1600.0 {
                    best_distance = distance;
                    base_best = Some(base);
                }
            }
        }
        if base_best.is_some() {
            return base_best;
        }

        for area in self.bwem.areas() {
            for base in area.bases() {
                if base.is_starting() {
                    continue;
                }
                let distance = self.ground_distance(main.center(), base.center());
                if distance < best_distance {
                    best_distance = distance;
                    base_best = Some(base);
                }
            }
        }
        base_best
    }

    /// The constructor of `Station` without the reserves, which the stations
    /// get once all are found: `initialize`, `findChoke`, `findAngles`,
    /// `findSecondaryLocations`, `findDefenses` and `cleanup`.
    fn station(&mut self, base: Base<'m>, main: bool, natural: bool) -> Station {
        // The resource centroid; the defense centroid is that of the minerals,
        // drawn halfway to each geyser in turn.
        let mut resource_centroid = Position::new(0, 0);
        let mut defense_centroid = Position::new(0, 0);
        let mut count = 0;
        for mineral in base.minerals() {
            resource_centroid = resource_centroid + mineral.position();
            count += 1;
        }
        if count > 0 {
            defense_centroid = resource_centroid / count;
        }
        for geyser in base.geysers() {
            defense_centroid = (defense_centroid + geyser.position()) / 2;
            resource_centroid = resource_centroid + geyser.position();
            count += 1;
        }
        if count > 0 {
            resource_centroid = resource_centroid / count;
        }
        let depot = self.depot;
        self.used.add_used(base.location(), &depot);

        let choke_point = self.find_choke_point(base, main, natural);
        if let (Some(cp), true) = (choke_point, main) {
            self.main_choke_points.push(cp.id());
        }
        let (base_angle, defense_angle) = self.find_angles(base, choke_point, main, natural);
        let (small_slot, medium_slot) =
            self.find_secondary_locations(base, main, base_angle, resource_centroid);
        let defenses = self.find_defenses(base, natural, defense_centroid);

        // `cleanup`: what the station marked used is free again.
        for &tile in &defenses {
            self.used.remove_used(tile, 2, 2);
        }
        self.used.remove_used(base.location(), 4, 3);
        if let Some(tile) = medium_slot {
            self.used.remove_used(tile, 3, 2);
        }
        if let Some(tile) = small_slot {
            self.used.remove_used(tile, 2, 2);
        }

        Station {
            base: base.id(),
            center: base.center(),
            choke_point: choke_point.map(|cp| cp.id()),
            is_main: main,
            is_natural: natural,
            resource_centroid,
            defense_angle,
            defenses: defenses.into_iter().collect(),
            small_slot,
            medium_slot,
        }
    }

    /// `Station::findAngles`: the angle from the base to its choke point and
    /// the angle defenses line up along, `(base angle, defense angle)`; both 0
    /// without a choke point.
    fn find_angles(
        &self,
        base: Base<'m>,
        choke_point: Option<ChokePoint<'m>>,
        main: bool,
        natural: bool,
    ) -> (f64, f64) {
        let Some(cp) = choke_point else {
            return (0.0, 0.0);
        };
        let angle_position = Position::from(cp.center()) + Position::new(4, 4);
        let distance = base.center().distance(angle_position);
        let ends = (
            Position::from(cp.node(Node::End1)),
            Position::from(cp.node(Node::End2)),
        );
        let base_angle =
            math::round(angle(base.center(), angle_position) / QUARTER_PI) * QUARTER_PI;
        let choke_angle = math::round(angle(ends.0, ends.1) / QUARTER_PI) * QUARTER_PI;
        let mut defense_angle = base_angle + HALF_PI;

        // Narrow choke points do not set the angles.
        if f64::from(cp.width()) < 96.0 {
            return (base_angle, defense_angle);
        }

        // A choke point against the left or right edge of the map is not angled
        // (a fix for Matchpoint).
        let x = TilePosition::from(cp.center()).x;
        if x < 10 || x > self.bwem.size().x - 10 {
            let choke_angle = math::round(angle(ends.0, ends.1) / HALF_PI) * HALF_PI;
            return (base_angle, choke_angle);
        }

        let area = base.area();
        if area.choke_points().len() >= 3 {
            // The last other choke point between the same two areas.
            let (a, b) = cp.areas();
            let mut second = None;
            for other in area.choke_points() {
                if other.id() == cp.id() {
                    continue;
                }
                let (c, d) = other.areas();
                if (a == c && b == d) || (a == d && b == c) {
                    second = Some(other);
                }
            }
            if let Some(second) = second {
                let across = angle(Position::from(cp.center()), Position::from(second.center()));
                defense_angle = (across + angle(ends.0, ends.1)) / 2.0;
            }
        } else if (distance >= 320.0 && !main && !natural) || (distance >= 368.0 && natural) {
            let base_remainder = math::fmod(base_angle + HALF_PI, PI);
            let choke_remainder = math::fmod(choke_angle, PI);
            defense_angle = math::fmod((base_remainder + choke_remainder) / 2.0, PI);
        }
        (base_angle, defense_angle)
    }

    /// `Station::findSecondaryLocations`: `(small, medium)`. A Terran base keeps
    /// the slot of its ComSat; a Zerg main tries the slots of a Spire and a
    /// Spawning Pool by its geyser, where a Sunken Colony also fits. Both are
    /// marked used, the ComSat as a Missile Turret.
    fn find_secondary_locations(
        &mut self,
        base: Base<'m>,
        main: bool,
        base_angle: f64,
        resource_centroid: Position,
    ) -> (Option<TilePosition>, Option<TilePosition>) {
        let here = base.location();
        if self.race == Race::TERRAN {
            let scanner = here + TilePosition::new(4, 1);
            let defense = self.defense;
            self.used.add_used(scanner, &defense);
            return (Some(scanner), None);
        }
        if self.race != Race::ZERG || !main {
            return (None, None);
        }

        let t = TilePosition::new;
        let (mut small, mut medium) = (None, None);
        let minerals_left = resource_centroid.x < base.center().x;
        let north = (0.785..2.355).contains(&base_angle);
        let west = (2.355..3.925).contains(&base_angle);
        let south = (3.925..5.495).contains(&base_angle);
        let geysers: Vec<TilePosition> = base.geysers().map(|g| g.top_left()).collect();
        for geyser in geysers {
            // (medium, small, defense) from the base location, the best first.
            let mut try_order: Vec<(TilePosition, TilePosition, TilePosition)> = Vec::new();

            // A geyser west of the hatchery.
            if geyser == here + t(-7, 1) {
                if north {
                    try_order = vec![(t(-3, -2), t(-5, -1), t(0, 0))];
                } else if west && !minerals_left {
                    try_order = vec![(t(-3, -4), t(-5, -2), t(-3, -2))];
                }
            }

            // A geyser north of the hatchery.
            if geyser == here + t(0, -5) {
                if north {
                    try_order = if minerals_left {
                        vec![
                            (t(-3, -4), t(0, -3), t(-2, -2)),
                            (t(0, -3), t(-2, -4), t(-2, -2)),
                        ]
                    } else {
                        vec![
                            (t(4, -4), t(0, -3), t(4, -2)),
                            (t(-1, -3), t(4, -4), t(4, -2)),
                        ]
                    };
                } else if west {
                    if !minerals_left {
                        try_order = vec![(t(-3, -4), t(-2, -2), t(0, -3))];
                    }
                } else if south {
                    try_order = if minerals_left {
                        vec![(t(-3, 5), t(1, 3), t(-1, 3)), (t(-2, 5), t(1, 3), t(-1, 3))]
                    } else {
                        vec![
                            (t(4, 5), t(1, 3), t(3, 3)),
                            (t(3, 5), t(1, 3), t(3, 3)),
                            (t(2, 5), t(1, 3), t(3, 3)),
                            (t(1, 5), t(1, 3), t(3, 3)),
                        ]
                    };
                } else if minerals_left {
                    // East.
                    try_order = vec![(t(4, -4), t(4, -2), t(0, -3))];
                }
            }

            let pool = known(UnitType::ZERG_SPAWNING_POOL);
            let spire = known(UnitType::ZERG_SPIRE);
            let sunken = known(UnitType::ZERG_SUNKEN_COLONY);
            for (m, s, d) in try_order {
                if self.used.can_place(self.bwem, &pool, here + m)
                    && self.used.can_place(self.bwem, &spire, here + s)
                    && self.used.can_place(self.bwem, &sunken, here + d)
                {
                    medium = Some(here + m);
                    small = Some(here + s);
                    self.used.add_used(here + s, &spire);
                    self.used.add_used(here + m, &pool);
                    break;
                }
            }
        }
        (small, medium)
    }

    /// `Station::findDefenses`: the defenses of the race at three corners of
    /// the depot away from the minerals and, by a lone geyser above or below a
    /// main or another base, at its sides; each that fits, marked used.
    fn find_defenses(
        &mut self,
        base: Base<'m>,
        natural: bool,
        defense_centroid: Position,
    ) -> BTreeSet<TilePosition> {
        let here = base.location();
        let mut placements = Vec::from([
            TilePosition::new(-2, -2),
            TilePosition::new(-2, 1),
            TilePosition::new(1, -2),
        ]);

        // Flipped away from the minerals.
        if base.center().y < defense_centroid.y {
            for p in &mut placements {
                p.y = -(p.y - 1);
            }
        }
        if base.center().x < defense_centroid.x {
            for p in &mut placements {
                p.x = -(p.x - 2);
            }
        }

        // A geyser above or below, as it should be: a defense on each side of it.
        let mut geysers = base.geysers();
        if let (Some(geyser), None, false) = (geysers.next(), geysers.next(), natural) {
            let tile = geyser.top_left();
            if tile.y < here.y - 3 || tile.y > here.y + 5 {
                let placement = tile - here;
                placements.push(placement + TilePosition::new(-2, 1));
                placements.push(placement + TilePosition::new(4, 1));

                // Off the way of the workers.
                for spot in &mut placements {
                    if *spot == TilePosition::new(2, -2) {
                        *spot = TilePosition::new(0, -2);
                    }
                }
            }
        }

        let defense = self.defense;
        let mut defenses = BTreeSet::new();
        for placement in placements {
            let tile = here + placement;
            if self.used.can_place(self.bwem, &defense, tile) {
                defenses.insert(tile);
                self.used.add_used(tile, &defense);
            }
        }
        defenses
    }

    /// `Station::findChoke`.
    fn find_choke_point(
        &self,
        base: Base<'m>,
        main: bool,
        natural: bool,
    ) -> Option<ChokePoint<'m>> {
        let area = base.area();
        // Only mains, naturals and bases with one choke point get one.
        if !main && !natural && area.choke_points().len() > 1 {
            return None;
        }

        // The nearest partner by ground: a natural for a main, a main otherwise.
        let mut partner = None;
        let mut best_distance = f64::MAX;
        let partners = if main {
            &self.natural_bases
        } else {
            &self.main_bases
        };
        for &candidate in partners {
            let distance = self.ground_distance(candidate.center(), base.center());
            if distance < best_distance {
                partner = Some(candidate);
                best_distance = distance;
            }
        }
        let partner = partner?;

        // No choke point when unreachable or in one area.
        let path: Vec<ChokePoint<'m>> = self
            .bwem
            .choke_point_path(partner.center(), base.center())
            .map(|p| p.iter().collect())
            .unwrap_or_default();
        if main && !path.is_empty() {
            if partner.area().choke_points().len() == 1 {
                // The natural is a pocket: the path shares its choke point; take
                // the other way out of the main if there are at most two.
                let partner_choke = path.last().map(|cp| cp.id());
                if area.choke_points().len() <= 2 {
                    if let Some(cp) = area.choke_points().find(|c| Some(c.id()) != partner_choke) {
                        return Some(cp);
                    }
                }
            } else {
                return path.first().copied();
            }
        }

        if area.choke_points().len() == 1 {
            return area.choke_points().next();
        }

        // The neighbouring area toward the center of the map, avoiding areas on
        // the path to the partner and areas behind narrow choke points.
        let mut second: Option<Area<'m>> = None;
        let mut best_distance = f64::MAX;
        for neighbour in area.accessible_neighbours() {
            let distance = Position::from(neighbour.top()).distance(self.bwem.center());
            let wrong_area = neighbour.choke_points().any(|c| {
                !self.nonsense_choke_points.contains(&c.id())
                    && ((!c.is_blocked() && c.node(Node::End1).distance(c.node(Node::End2)) <= 2.0)
                        || path.iter().any(|p| p.id() == c.id()))
            });
            if wrong_area {
                continue;
            }
            if distance < best_distance {
                second = Some(neighbour);
                best_distance = distance;
            }
        }

        let mut choke_point = None;
        let mut best_distance = f64::MAX;
        for c in area.choke_points() {
            let (a, b) = c.areas();
            if c.is_blocked()
                || self.main_choke_points.contains(&c.id())
                || c.geometry().len() <= 3
                || (Some(a) != second && Some(b) != second)
            {
                continue;
            }
            let distance = Position::from(c.center()).distance(partner.center());
            if distance < best_distance {
                choke_point = Some(c);
                best_distance = distance;
            }
        }
        choke_point
    }
}

/// `Station::addResourceReserves`: the tiles of each resource, and a line of
/// tiles from the depot to it.
fn add_resource_reserves(bwem: &bwem::Map, layout: &mut Layout, base: Base<'_>) {
    let resources = base
        .minerals()
        .map(|m| (m.top_left(), m.size(), TilePosition::new(2, 1)))
        .chain(
            base.geysers()
                .map(|g| (g.top_left(), g.size(), TilePosition::new(4, 2))),
        );
    for (top_left, size, reserved) in resources {
        layout.add_reserve(top_left, reserved.x, reserved.y);
        let (start, end) = closest_tiles(base.location(), top_left, size);
        bigger_reserve(bwem, layout, start, end);
    }
}

/// Reserves tiles from `start` to `end`, each step to the neighbour nearest to
/// `end`.
fn bigger_reserve(bwem: &bwem::Map, layout: &mut Layout, start: TilePosition, end: TilePosition) {
    const DIRECTIONS: [TilePosition; 4] = [
        TilePosition::new(1, 0),
        TilePosition::new(-1, 0),
        TilePosition::new(0, 1),
        TilePosition::new(0, -1),
    ];
    let mut next = start;
    let end_position = Position::from(end);
    while next.distance(end) > 1.0 {
        let mut best_distance = f64::MAX;
        let start = next;
        for d in DIRECTIONS {
            let tile = start + d;
            if !bwem.valid(tile) {
                continue;
            }
            let distance = Position::from(tile).distance(end_position);
            if distance <= best_distance {
                next = tile;
                best_distance = distance;
            }
        }
        layout.add_reserve(next, 1, 1);
    }
}

/// The tiles of a depot at `location` and of a resource that face each other:
/// the nearest edges where the two are apart, the middles where they overlap.
fn closest_tiles(
    location: TilePosition,
    resource: TilePosition,
    size: TilePosition,
) -> (TilePosition, TilePosition) {
    let (ax1, ay1) = (location.x, location.y);
    let (ax2, ay2) = (ax1 + 3, ay1 + 2);
    let (bx1, by1) = (resource.x, resource.y);
    let (bx2, by2) = (bx1 + size.x - 1, by1 + size.y - 1);

    let (mut tile_ax, mut tile_ay) = ((ax1 + ax2) / 2, (ay1 + ay2) / 2);
    let (mut tile_bx, mut tile_by) = ((bx1 + bx2) / 2, (by1 + by2) / 2);
    if ax2 < bx1 {
        tile_ax = ax2;
        tile_bx = bx1;
    } else if bx2 < ax1 {
        tile_ax = ax1;
        tile_bx = bx2;
    }
    if ay2 < by1 {
        tile_ay = ay2;
        tile_by = by1;
    } else if by2 < ay1 {
        tile_ay = ay1;
        tile_by = by2;
    }
    (
        TilePosition::new(tile_ax, tile_ay),
        TilePosition::new(tile_bx, tile_by),
    )
}
