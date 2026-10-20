// Port of BWEM/include/base.h and BWEM/src/base.cpp of BWEM-community
// (https://github.com/N00byEdge/BWEM-community, 9a63141).
// Copyright (c) 2015, 2017, Igor Dimitrijevic. MIT/X11 License, see LICENSE-BWEM.

//! Bases: locations for a resource depot near resources.

use alloc::vec::Vec;

use bwapi::{Position, TilePosition};

use super::map::MapView;
use super::{Area, BaseId, Neutral, NeutralId};

/// `UnitType(Terran_Command_Center).tileSize()`: the footprint BWEM places bases
/// with.
pub(crate) const COMMAND_CENTER_SIZE: TilePosition = TilePosition::new(4, 3);

/// What never changes in a base after the analysis; [`Base`] is its view.
#[derive(Clone, Debug)]
pub(crate) struct StaticBase {
    pub(crate) area: i16,
    pub(crate) location: TilePosition,
    pub(crate) center: Position,
    pub(crate) geysers: Vec<u32>,
    pub(crate) starting: bool,
    /// Its minerals as of the analysis.
    pub(crate) initial_minerals: Vec<NeutralId>,
}

/// What changes in a base: its minerals not mined out.
#[derive(Clone, Debug)]
pub(crate) struct BaseState {
    pub(crate) minerals: Vec<u32>,
    pub(crate) blocking_minerals: Vec<u32>,
}

impl StaticBase {
    pub(crate) fn new(area: i16, location: TilePosition, geysers: Vec<u32>) -> Self {
        StaticBase {
            area,
            location,
            center: base_center(location),
            geysers,
            starting: false,
            initial_minerals: Vec::new(),
        }
    }

    /// `Base::SetStartingLocation`.
    pub(crate) fn set_starting_location(&mut self, actual_location: TilePosition) {
        self.starting = true;
        self.location = actual_location;
        self.center = base_center(actual_location);
    }
}

impl BaseState {
    /// `Base::OnMineralDestroyed`.
    pub(crate) fn on_mineral_destroyed(&mut self, mineral: u32) {
        if let Some(i) = self.minerals.iter().position(|&m| m == mineral) {
            self.minerals.swap_remove(i);
        }
        if let Some(i) = self.blocking_minerals.iter().position(|&m| m == mineral) {
            self.blocking_minerals.swap_remove(i);
        }
        // The other copies too: `ValidateBaseLocation` lists a mineral once per
        // tile, and BWEM keeps the second copy, a dangling pointer. As the
        // reference, with a local patch.
        self.blocking_minerals.retain(|&m| m != mineral);
    }
}

fn base_center(location: TilePosition) -> Position {
    Position::from(location) + Position::from(COMMAND_CENTER_SIZE) / 2
}

/// A base: where a resource depot can stand near a group of resources.
#[derive(Clone, Copy)]
pub struct Base<'m> {
    map: MapView<'m>,
    index: u32,
}

impl core::fmt::Debug for Base<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("Base").field(&self.index).finish()
    }
}

impl<'m> Base<'m> {
    pub(crate) fn new(map: MapView<'m>, index: u32) -> Self {
        Base { map, index }
    }

    fn data(self) -> &'m StaticBase {
        &self.map.geometry.bases[self.index as usize]
    }

    fn state(self) -> &'m BaseState {
        &self.map.state.bases[self.index as usize]
    }

    pub fn id(self) -> BaseId {
        BaseId(self.index)
    }

    /// Whether a start location was assigned to the base by
    /// [`Map::find_bases_for_starting_locations`](super::Map::find_bases_for_starting_locations).
    pub fn is_starting(self) -> bool {
        self.data().starting
    }

    pub fn area(self) -> Area<'m> {
        Area::new(self.map, self.data().area)
    }

    /// The top left tile of the depot.
    pub fn location(self) -> TilePosition {
        self.data().location
    }

    /// The center of the depot.
    pub fn center(self) -> Position {
        self.data().center
    }

    /// Its minerals not mined out, in the order BWEM keeps them.
    pub fn minerals(self) -> impl Iterator<Item = Neutral<'m>> + 'm {
        let map = self.map;
        self.state()
            .minerals
            .iter()
            .map(move |&i| Neutral::new(map, i))
    }

    /// Its minerals as of the analysis.
    pub fn initial_minerals(self) -> &'m [NeutralId] {
        &self.data().initial_minerals
    }

    pub fn geysers(self) -> impl Iterator<Item = Neutral<'m>> + 'm {
        let map = self.map;
        self.data()
            .geysers
            .iter()
            .map(move |&i| Neutral::new(map, i))
    }

    /// Small minerals (at most 8) inside the depot or close to it, not mined
    /// out yet: they must be before the depot can be built.
    pub fn blocking_minerals(self) -> impl Iterator<Item = Neutral<'m>> + 'm {
        let map = self.map;
        self.state()
            .blocking_minerals
            .iter()
            .map(move |&i| Neutral::new(map, i))
    }
}
