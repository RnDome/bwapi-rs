// Port of BWEM/include/neutral.h and BWEM/src/neutral.cpp of BWEM-community
// (https://github.com/N00byEdge/BWEM-community, 9a63141).
// Copyright (c) 2015, 2017, Igor Dimitrijevic. MIT/X11 License, see LICENSE-BWEM.

//! Static neutrals: minerals, geysers and static buildings.

use alloc::vec::Vec;
use core::ops::Deref;

use bwapi::{Position, TilePosition, WalkPosition};

use super::{Area, Map, NeutralId, NeutralKind};

/// The data of a neutral; [`Neutral`] is its view.
#[derive(Clone, Debug)]
pub(crate) struct NeutralData {
    pub(crate) kind: NeutralKind,
    pub(crate) unit_id: i32,
    pub(crate) type_id: i32,
    pub(crate) pos: Position,
    pub(crate) top_left: TilePosition,
    pub(crate) size: TilePosition,
    pub(crate) initial_amount: i32,
    pub(crate) next_stacked: Option<u32>,
    /// One mini tile per area the neutral separates: its true doors.
    pub(crate) blocked_areas: Vec<WalkPosition>,
    pub(crate) alive: bool,
}

/// A static neutral: a mineral, a geyser or a static building.
#[derive(Clone, Copy)]
pub struct Neutral<'m> {
    map: &'m Map,
    index: u32,
}

impl core::fmt::Debug for Neutral<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("Neutral").field(&self.index).finish()
    }
}

impl<'m> Neutral<'m> {
    pub(crate) fn new(map: &'m Map, index: u32) -> Self {
        Neutral { map, index }
    }

    fn data(self) -> &'m NeutralData {
        &self.map.neutrals[self.index as usize]
    }

    pub fn id(self) -> NeutralId {
        NeutralId(self.index)
    }

    /// `IsRessource`.
    pub fn as_resource(self) -> Option<Resource<'m>> {
        (self.data().kind != NeutralKind::StaticBuilding).then_some(Resource(self))
    }

    /// `IsMineral`.
    pub fn as_mineral(self) -> Option<Mineral<'m>> {
        (self.data().kind == NeutralKind::Mineral).then_some(Mineral(self))
    }

    /// `IsGeyser`.
    pub fn as_geyser(self) -> Option<Geyser<'m>> {
        (self.data().kind == NeutralKind::Geyser).then_some(Geyser(self))
    }

    /// `IsStaticBuilding`.
    pub fn as_static_building(self) -> Option<StaticBuilding<'m>> {
        (self.data().kind == NeutralKind::StaticBuilding).then_some(StaticBuilding(self))
    }

    /// The id of the unit: in a game, `game.unit(id)`.
    pub fn unit_id(self) -> i32 {
        self.data().unit_id
    }

    /// The id of the unit type: in a game, `UnitType::from_id(id)`.
    pub fn type_id(self) -> i32 {
        self.data().type_id
    }

    /// The initial position of the unit.
    pub fn pos(self) -> Position {
        self.data().pos
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
    pub fn blocking(self) -> bool {
        !self.data().blocked_areas.is_empty()
    }

    /// The areas the neutral separates, nearest to each of its true doors.
    pub fn blocked_areas(self) -> impl Iterator<Item = Area<'m>> + 'm {
        let map = self.map;
        self.data()
            .blocked_areas
            .iter()
            .filter_map(move |&w| map.nearest_area(w))
    }

    /// The neutral stacked on this one.
    pub fn next_stacked(self) -> Option<Neutral<'m>> {
        self.data().next_stacked.map(|i| Neutral::new(self.map, i))
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

/// A mineral or a geyser: `Ressource`.
#[derive(Clone, Copy)]
pub struct Resource<'m>(pub(crate) Neutral<'m>);

impl Resource<'_> {
    pub fn initial_amount(self) -> i32 {
        self.0.data().initial_amount
    }
}

impl<'m> Deref for Resource<'m> {
    type Target = Neutral<'m>;

    fn deref(&self) -> &Neutral<'m> {
        &self.0
    }
}

/// A mineral field.
#[derive(Clone, Copy)]
pub struct Mineral<'m>(pub(crate) Neutral<'m>);

impl Mineral<'_> {
    pub fn initial_amount(self) -> i32 {
        self.0.data().initial_amount
    }
}

impl<'m> Deref for Mineral<'m> {
    type Target = Neutral<'m>;

    fn deref(&self) -> &Neutral<'m> {
        &self.0
    }
}

/// A vespene geyser.
#[derive(Clone, Copy)]
pub struct Geyser<'m>(pub(crate) Neutral<'m>);

impl Geyser<'_> {
    pub fn initial_amount(self) -> i32 {
        self.0.data().initial_amount
    }
}

impl<'m> Deref for Geyser<'m> {
    type Target = Neutral<'m>;

    fn deref(&self) -> &Neutral<'m> {
        &self.0
    }
}

/// A special building or a pit door.
#[derive(Clone, Copy)]
pub struct StaticBuilding<'m>(pub(crate) Neutral<'m>);

impl<'m> Deref for StaticBuilding<'m> {
    type Target = Neutral<'m>;

    fn deref(&self) -> &Neutral<'m> {
        &self.0
    }
}

impl Map {
    /// `Neutral::PutOnTiles`: a neutral on a tile that has one joins the top of
    /// that stack.
    pub(crate) fn put_on_tiles(&mut self, index: u32) {
        let NeutralData { top_left, size, .. } = self.neutrals[index as usize];
        for dy in 0..size.y {
            for dx in 0..size.x {
                let t = top_left + TilePosition::new(dx, dy);
                match self.tiles[t].neutral {
                    None => self.tiles[t].neutral = Some(index),
                    Some(bottom) => {
                        let top = self.last_stacked(bottom);
                        self.neutrals[top as usize].next_stacked = Some(index);
                        return;
                    }
                }
            }
        }
    }

    /// `Neutral::RemoveFromTiles`.
    pub(crate) fn remove_from_tiles(&mut self, index: u32) {
        let NeutralData {
            top_left,
            size,
            next_stacked,
            ..
        } = self.neutrals[index as usize];
        for dy in 0..size.y {
            for dx in 0..size.x {
                let t = top_left + TilePosition::new(dx, dy);
                match self.tiles[t].neutral {
                    Some(n) if n == index => self.tiles[t].neutral = next_stacked,
                    Some(bottom) => {
                        let mut prev = bottom;
                        while let Some(next) = self.neutrals[prev as usize].next_stacked {
                            if next == index {
                                break;
                            }
                            prev = next;
                        }
                        if self.neutrals[prev as usize].next_stacked == Some(index) {
                            self.neutrals[prev as usize].next_stacked = next_stacked;
                        }
                        self.neutrals[index as usize].next_stacked = None;
                        return;
                    }
                    None => {}
                }
            }
        }
        self.neutrals[index as usize].next_stacked = None;
    }

    fn last_stacked(&self, mut index: u32) -> u32 {
        while let Some(next) = self.neutrals[index as usize].next_stacked {
            index = next;
        }
        index
    }
}
