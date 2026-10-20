// Port of BWEM/include/cp.h and BWEM/src/cp.cpp of BWEM-community
// (https://github.com/N00byEdge/BWEM-community, 9a63141).
// Copyright (c) 2015, 2017, Igor Dimitrijevic. MIT/X11 License, see LICENSE-BWEM.

//! Choke points: the frontiers between two areas.

use alloc::vec::Vec;

use bwapi::WalkPosition;

use super::{Area, ChokePointId, Map, Neutral};
use crate::PositionExt;

/// A node of a choke point: `ChokePoint::node`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Node {
    End1,
    Middle,
    End2,
}

impl Node {
    fn index(self) -> usize {
        match self {
            Node::End1 => 0,
            Node::Middle => 1,
            Node::End2 => 2,
        }
    }
}

/// The data of a choke point; [`ChokePoint`] is its view.
#[derive(Clone, Debug)]
pub(crate) struct ChokePointData {
    pub(crate) areas: (i16, i16),
    pub(crate) nodes: [WalkPosition; 3],
    pub(crate) nodes_in_area: [(WalkPosition, WalkPosition); 3],
    pub(crate) geometry: Vec<WalkPosition>,
    pub(crate) blocked: bool,
    /// The bottom neutral of the blocking stack.
    pub(crate) blocking_neutral: Option<u32>,
    pub(crate) pseudo: bool,
}

/// A choke point between two areas: a frontier of mini tiles, or a pseudo choke
/// point standing for a blocking neutral.
#[derive(Clone, Copy)]
pub struct ChokePoint<'m> {
    map: &'m Map,
    index: u32,
}

impl core::fmt::Debug for ChokePoint<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("ChokePoint").field(&self.index).finish()
    }
}

impl<'m> ChokePoint<'m> {
    pub(crate) fn new(map: &'m Map, index: u32) -> Self {
        ChokePoint { map, index }
    }

    fn data(self) -> &'m ChokePointData {
        &self.map.choke_points[self.index as usize]
    }

    /// Its index in the distance matrix.
    pub fn id(self) -> ChokePointId {
        ChokePointId(self.index)
    }

    /// A pseudo choke point stands for a blocking neutral.
    pub fn is_pseudo(self) -> bool {
        self.data().pseudo
    }

    pub fn areas(self) -> (Area<'m>, Area<'m>) {
        let (a, b) = self.data().areas;
        (Area::new(self.map, a), Area::new(self.map, b))
    }

    /// `pos(Node::Middle)`.
    pub fn center(self) -> WalkPosition {
        self.pos(Node::Middle)
    }

    /// The ends of the frontier, and its middle: the point of highest altitude
    /// near the middle of the frontier.
    pub fn pos(self, n: Node) -> WalkPosition {
        self.data().nodes[n.index()]
    }

    /// The mini tile of `area` nearest to the node, not under a neutral. For an
    /// area that is not one of the two, the position in the second.
    pub fn pos_in_area(self, n: Node, area: Area<'_>) -> WalkPosition {
        let (first, second) = self.data().nodes_in_area[n.index()];
        if area.id().get() == self.data().areas.0 {
            first
        } else {
            second
        }
    }

    /// The frontier from one end to the other.
    pub fn geometry(self) -> &'m [WalkPosition] {
        &self.data().geometry
    }

    /// A pseudo choke point is blocked until its neutral is destroyed and the
    /// map updates paths automatically.
    pub fn blocked(self) -> bool {
        self.data().blocked
    }

    /// With stacked neutrals, the bottom one.
    pub fn blocking_neutral(self) -> Option<Neutral<'m>> {
        self.data()
            .blocking_neutral
            .map(|i| Neutral::new(self.map, i))
    }

    /// Ground distance in pixels, through areas and choke points; `None` when
    /// unreachable (-1 in BWEM).
    pub fn distance_from(self, cp: ChokePoint<'_>) -> Option<i32> {
        let d = self.map.cp_distance(self.index, cp.index);
        (d != -1).then_some(d)
    }

    pub fn accessible_from(self, cp: ChokePoint<'_>) -> bool {
        self.map.cp_distance(self.index, cp.index) >= 0
    }

    /// The choke points from this one to `cp` through the graph of choke points,
    /// both included.
    pub fn choke_point_path_to(self, cp: ChokePoint<'_>) -> CpPath<'m> {
        CpPath::new(self.map, self.map.cp_path(self.index, cp.index))
    }

    pub fn map(self) -> &'m Map {
        self.map
    }

    /// The distance between the two ends in whole mini tiles, in pixels. Not in
    /// BWEM-community: BWEB calls `Width()` of the fork of BWEM in McRave
    /// (Christian McCrave, MIT), which truncates the distance in mini tiles
    /// before scaling it.
    pub fn width(self) -> i32 {
        self.pos(Node::End1).distance(self.pos(Node::End2)) as i32 * 8
    }
}

impl Map {
    /// `ChokePoint::OnBlockingNeutralDestroyed`, after the neutral left its
    /// tiles: the choke point keeps the next neutral of the stack, and opens
    /// when none is left and paths update automatically.
    pub(crate) fn cp_on_blocking_neutral_destroyed(&mut self, cp: u32, blocking: u32) {
        if self.choke_points[cp as usize].blocking_neutral != Some(blocking) {
            return;
        }
        let next = self.tiles[self.neutrals[blocking as usize].top_left].neutral;
        let data = &mut self.choke_points[cp as usize];
        data.blocking_neutral = next;
        if next.is_none() && self.automatic_path_update {
            data.blocked = false;
        }
    }
}

/// A path of choke points: `CPPath`.
#[derive(Clone, Copy)]
pub struct CpPath<'m> {
    map: &'m Map,
    cps: &'m [u32],
}

impl<'m> CpPath<'m> {
    pub(crate) fn new(map: &'m Map, cps: &'m [u32]) -> Self {
        CpPath { map, cps }
    }

    pub fn len(self) -> usize {
        self.cps.len()
    }

    pub fn is_empty(self) -> bool {
        self.cps.is_empty()
    }

    pub fn get(self, i: usize) -> Option<ChokePoint<'m>> {
        self.cps.get(i).map(|&cp| ChokePoint::new(self.map, cp))
    }

    pub fn first(self) -> Option<ChokePoint<'m>> {
        self.get(0)
    }

    pub fn last(self) -> Option<ChokePoint<'m>> {
        self.cps.last().map(|&cp| ChokePoint::new(self.map, cp))
    }

    pub fn iter(self) -> impl DoubleEndedIterator<Item = ChokePoint<'m>> + ExactSizeIterator + 'm {
        let map = self.map;
        self.cps.iter().map(move |&cp| ChokePoint::new(map, cp))
    }
}
