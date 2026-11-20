// Port of BWEM/include/cp.h and BWEM/src/cp.cpp of BWEM-community
// (https://github.com/N00byEdge/BWEM-community, 9a63141).
// Copyright (c) 2015, 2017, Igor Dimitrijevic. MIT/X11 License, see LICENSE-BWEM.

//! Choke points: the frontiers between two areas.

use alloc::vec::Vec;

use bwapi::WalkPosition;

use super::map::MapView;
use super::{Area, AreaId, ChokePointId, Map, Neutral};
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

/// What never changes in a choke point; [`ChokePoint`] is its view.
#[derive(Clone, Debug)]
pub(crate) struct StaticChokePoint {
    pub(crate) areas: (i16, i16),
    pub(crate) nodes: [WalkPosition; 3],
    pub(crate) nodes_in_area: [(WalkPosition, WalkPosition); 3],
    pub(crate) geometry: Vec<WalkPosition>,
    pub(crate) pseudo: bool,
}

/// What changes in a choke point when its blocking neutrals are destroyed.
#[derive(Clone, Debug)]
pub(crate) struct ChokePointState {
    pub(crate) blocked: bool,
    /// The bottom neutral of the blocking stack.
    pub(crate) blocking_neutral: Option<u32>,
}

/// A choke point between two areas: a frontier of mini tiles, or a pseudo choke
/// point standing for a blocking neutral.
#[derive(Clone, Copy)]
pub struct ChokePoint<'m> {
    map: MapView<'m>,
    index: u32,
}

impl core::fmt::Debug for ChokePoint<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("ChokePoint").field(&self.index).finish()
    }
}

impl<'m> ChokePoint<'m> {
    pub(crate) fn new(map: MapView<'m>, index: u32) -> Self {
        ChokePoint { map, index }
    }

    fn data(self) -> &'m StaticChokePoint {
        &self.map.geometry.choke_points[self.index as usize]
    }

    fn state(self) -> &'m ChokePointState {
        &self.map.state.choke_points[self.index as usize]
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

    /// `node(Node::Middle)`.
    pub fn center(self) -> WalkPosition {
        self.node(Node::Middle)
    }

    /// The ends of the frontier, and its middle: the point of highest altitude
    /// near the middle of the frontier. `Pos`.
    pub fn node(self, node: Node) -> WalkPosition {
        self.data().nodes[node.index()]
    }

    /// The mini tile of `area` nearest to the node, not under a neutral. For an
    /// area that is not one of the two, the position in the second. `PosInArea`.
    pub fn node_in_area(self, node: Node, area: AreaId) -> WalkPosition {
        let (first, second) = self.data().nodes_in_area[node.index()];
        if area.get() == self.data().areas.0 {
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
    pub fn is_blocked(self) -> bool {
        self.state().blocked
    }

    /// With stacked neutrals, the bottom one.
    pub fn blocking_neutral(self) -> Option<Neutral<'m>> {
        self.state()
            .blocking_neutral
            .map(|i| Neutral::new(self.map, i))
    }

    /// Ground distance in pixels, through areas and choke points; `None` when
    /// unreachable (-1 in BWEM).
    pub fn distance_to(self, other: ChokePointId) -> Option<i32> {
        let distance = self.map.choke_point_distance(self.index, other.0);
        (distance != -1).then_some(distance)
    }

    pub fn is_reachable_from(self, other: ChokePointId) -> bool {
        self.map.choke_point_distance(self.index, other.0) >= 0
    }

    /// The choke points from this one to `other` through the graph of choke
    /// points, both included; to itself, this one alone. `None` when BWEM keeps
    /// no path. As `GetPath`, a path may come without a length: BWEM can join
    /// two choke points it gives no distance between.
    pub fn path_to(self, other: ChokePointId) -> Option<ChokePointPath<'m>> {
        let choke_points = self.map.choke_point_path_between(self.index, other.0);
        (!choke_points.is_empty())
            .then(|| ChokePointPath::new(self.map, choke_points, self.distance_to(other)))
    }

    /// The distance between the two ends in whole mini tiles, in pixels. Not in
    /// BWEM-community: BWEB calls `Width()` of the fork of BWEM in McRave
    /// (Christian McCrave, MIT), which truncates the distance in mini tiles
    /// before scaling it.
    pub fn width(self) -> i32 {
        self.node(Node::End1).distance(self.node(Node::End2)) as i32 * 8
    }
}

impl Map {
    /// `ChokePoint::OnBlockingNeutralDestroyed`, after the neutral left its
    /// tiles: the choke point keeps the next neutral of the stack, and opens
    /// when none is left and paths update automatically.
    pub(crate) fn update_choke_point_after_neutral_removal(
        &mut self,
        choke_point: u32,
        blocking: u32,
    ) {
        let state = &mut self.state;
        if state.choke_points[choke_point as usize].blocking_neutral != Some(blocking) {
            return;
        }
        let next = state.tiles[self.geometry.neutrals[blocking as usize].top_left].neutral;
        let automatic_path_update = state.automatic_path_update;
        let choke_point_state = &mut state.choke_points[choke_point as usize];
        choke_point_state.blocking_neutral = next;
        if next.is_none() && automatic_path_update {
            choke_point_state.blocked = false;
        }
    }
}

/// A path of choke points and its length: `CPPath`.
#[derive(Clone, Copy)]
pub struct ChokePointPath<'m> {
    map: MapView<'m>,
    choke_points: &'m [u32],
    length: Option<i32>,
}

impl<'m> ChokePointPath<'m> {
    pub(crate) fn new(map: MapView<'m>, choke_points: &'m [u32], length: Option<i32>) -> Self {
        ChokePointPath {
            map,
            choke_points,
            length,
        }
    }

    /// The length of the route in pixels: 32 a straight step, 45 a diagonal
    /// one, as BWEM counts; between two positions, approximate where they
    /// meet the route. Not the number of choke points: that is
    /// [`ChokePointPath::len`]. `None` where BWEM gives -1.
    pub fn length(self) -> Option<i32> {
        self.length
    }

    /// The number of choke points on the route.
    pub fn len(self) -> usize {
        self.choke_points.len()
    }

    pub fn is_empty(self) -> bool {
        self.choke_points.is_empty()
    }

    pub fn get(self, i: usize) -> Option<ChokePoint<'m>> {
        self.choke_points
            .get(i)
            .map(|&choke_point| ChokePoint::new(self.map, choke_point))
    }

    pub fn first(self) -> Option<ChokePoint<'m>> {
        self.get(0)
    }

    pub fn last(self) -> Option<ChokePoint<'m>> {
        self.choke_points
            .last()
            .map(|&choke_point| ChokePoint::new(self.map, choke_point))
    }

    pub fn iter(self) -> impl DoubleEndedIterator<Item = ChokePoint<'m>> + ExactSizeIterator + 'm {
        let map = self.map;
        self.choke_points
            .iter()
            .map(move |&choke_point| ChokePoint::new(map, choke_point))
    }
}
