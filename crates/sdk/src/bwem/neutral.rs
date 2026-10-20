// Port of BWEM/include/neutral.h and BWEM/src/neutral.cpp of BWEM-community
// (https://github.com/N00byEdge/BWEM-community, 9a63141).
// Copyright (c) 2015, 2017, Igor Dimitrijevic. MIT/X11 License, see LICENSE-BWEM.

//! Static neutrals: minerals, geysers and static buildings.

use alloc::vec::Vec;

use bwapi::{Position, TilePosition, UnitType, WalkPosition};

use super::map::MapView;
use super::{Area, Map, NeutralId, NeutralKind};

/// What never changes in a neutral; [`Neutral`] is its view.
#[derive(Clone, Debug)]
pub(crate) struct StaticNeutral {
    pub(crate) kind: NeutralKind,
    pub(crate) unit_id: i32,
    pub(crate) type_id: i32,
    pub(crate) position: Position,
    pub(crate) top_left: TilePosition,
    pub(crate) size: TilePosition,
    pub(crate) initial_amount: i32,
    /// One mini tile per area the neutral separates: its true doors.
    pub(crate) blocked_areas: Vec<WalkPosition>,
}

/// What changes in a neutral: the stack it is in, and whether it is destroyed.
#[derive(Clone, Debug)]
pub(crate) struct NeutralState {
    pub(crate) next_stacked: Option<u32>,
    pub(crate) alive: bool,
}

/// A static neutral: a mineral, a geyser or a static building.
#[derive(Clone, Copy)]
pub struct Neutral<'m> {
    map: MapView<'m>,
    index: u32,
}

impl core::fmt::Debug for Neutral<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("Neutral").field(&self.index).finish()
    }
}

impl<'m> Neutral<'m> {
    pub(crate) fn new(map: MapView<'m>, index: u32) -> Self {
        Neutral { map, index }
    }

    fn data(self) -> &'m StaticNeutral {
        &self.map.geometry.neutrals[self.index as usize]
    }

    pub fn id(self) -> NeutralId {
        NeutralId(self.index)
    }

    /// A mineral, a geyser or a static building: `IsMineral`, `IsGeyser`,
    /// `IsStaticBuilding`.
    pub fn kind(self) -> NeutralKind {
        self.data().kind
    }

    /// Not destroyed.
    pub fn is_alive(self) -> bool {
        self.map.state.neutrals[self.index as usize].alive
    }

    /// The resources of a mineral or a geyser at the start; `None` for a static
    /// building.
    pub fn initial_amount(self) -> Option<i32> {
        (self.data().kind != NeutralKind::StaticBuilding).then_some(self.data().initial_amount)
    }

    /// The id of the unit: in a game, `game.unit(id)`.
    pub fn unit_id(self) -> i32 {
        self.data().unit_id
    }

    pub fn unit_type(self) -> UnitType {
        UnitType::from_id(self.data().type_id)
    }

    /// The initial position of the unit.
    pub fn position(self) -> Position {
        self.data().position
    }

    pub fn top_left(self) -> TilePosition {
        self.data().top_left
    }

    pub fn bottom_right(self) -> TilePosition {
        self.data().top_left + self.data().size - TilePosition::new(1, 1)
    }

    pub fn size(self) -> TilePosition {
        self.data().size
    }

    /// Whether the neutral separates areas: then its mini tiles are in no area
    /// and a pseudo choke point stands for it.
    pub fn is_blocking(self) -> bool {
        !self.data().blocked_areas.is_empty()
    }

    /// The areas the neutral separates, nearest to each of its true doors.
    pub fn blocked_areas(self) -> impl Iterator<Item = Area<'m>> + 'm {
        let map = self.map;
        self.data()
            .blocked_areas
            .iter()
            .filter_map(move |&w| map.nearest_area_id(w).map(|id| Area::new(map, id)))
    }

    /// The neutral stacked on this one.
    pub fn next_stacked(self) -> Option<Neutral<'m>> {
        self.map.state.neutrals[self.index as usize]
            .next_stacked
            .map(|i| Neutral::new(self.map, i))
    }

    /// The top neutral of the stack from this one up.
    pub fn last_stacked(self) -> Neutral<'m> {
        let mut top = self;
        while let Some(next) = top.next_stacked() {
            top = next;
        }
        top
    }
}

impl Map {
    /// `Neutral::RemoveFromTiles`.
    pub(crate) fn remove_from_tiles(&mut self, index: u32) {
        let StaticNeutral { top_left, size, .. } = self.geometry.neutrals[index as usize];
        let state = &mut self.state;
        let next_stacked = state.neutrals[index as usize].next_stacked;
        for dy in 0..size.y {
            for dx in 0..size.x {
                let t = top_left + TilePosition::new(dx, dy);
                match state.tiles[t].neutral {
                    Some(n) if n == index => state.tiles[t].neutral = next_stacked,
                    Some(bottom) => {
                        let mut prev = bottom;
                        while let Some(next) = state.neutrals[prev as usize].next_stacked {
                            if next == index {
                                break;
                            }
                            prev = next;
                        }
                        if state.neutrals[prev as usize].next_stacked == Some(index) {
                            state.neutrals[prev as usize].next_stacked = next_stacked;
                        }
                        state.neutrals[index as usize].next_stacked = None;
                        return;
                    }
                    None => {}
                }
            }
        }
        state.neutrals[index as usize].next_stacked = None;
    }
}
