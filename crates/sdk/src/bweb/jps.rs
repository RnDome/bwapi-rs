// Port of JPS.h of BWEB (https://github.com/Cmccrave/BWEB, v1.17, d57980c), the
// Jump Point Search of False.Genesis (https://github.com/fgenesis/jps), public
// domain. Attribution kept at the request of its author.

//! Jump Point Search on a uniform grid, as `JPS::findPath` with `step` 0 and
//! `skip` 1, the way BWEB calls it: without the incremental interface, the
//! step counters (with no limit they do not change the result) and the debug
//! checks.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use bwapi::TilePosition;

use crate::{math, stdsort};

/// `JPS::Position`: unsigned in C++, where a step off the map wraps around and
/// the grid says it is not walkable; here the grid gets the negative value.
type Pos = (i32, i32);

/// `Internal::Node`.
struct Node {
    f: u32,
    g: u32,
    pos: Pos,
    parent: Option<u32>,
    open: bool,
    closed: bool,
}

struct Searcher<G> {
    grid: G,
    /// The nodes, which C++ keeps in a `std::map` by position.
    nodes: Vec<Node>,
    index: BTreeMap<(i32, i32), u32>,
    /// A binary heap of node indices with the least `f` on top.
    open: Vec<u32>,
    end: u32,
}

/// The way points from `start` to `end`, without `start`; empty when they are
/// equal. `None` if there is no path. `grid(x, y)` tells whether a cell is
/// walkable, also off the grid.
pub(crate) fn find_path(
    grid: impl FnMut(i32, i32) -> bool,
    start: TilePosition,
    end: TilePosition,
) -> Option<Vec<TilePosition>> {
    let mut search = Searcher {
        grid,
        nodes: Vec::new(),
        index: BTreeMap::new(),
        open: Vec::new(),
        end: 0,
    };
    let (start, end) = ((start.x, start.y), (end.x, end.y));

    // `findPathInit`.
    if start == end {
        // The start is not part of the path, so there is nothing to find.
        return search.walkable(end.0, end.1).then(Vec::new);
    }
    if !search.walkable(start.0, start.1) || !search.walkable(end.0, end.1) {
        return None;
    }
    search.end = search.node(end);
    let start_node = search.node(start);
    if !search.find_path_greedy(start_node) {
        search.push_open(start_node);
        // `findPathStep`.
        loop {
            let n = search.pop_open()?;
            search.nodes[n as usize].closed = true;
            if n == search.end {
                break;
            }
            search.identify_successors(n);
        }
    }
    search.generate_path()
}

impl<G: FnMut(i32, i32) -> bool> Searcher<G> {
    fn walkable(&mut self, x: i32, y: i32) -> bool {
        (self.grid)(x, y)
    }

    /// `getNode`: the node at `pos`, created on first use.
    fn node(&mut self, pos: Pos) -> u32 {
        let next = self.nodes.len() as u32;
        let index = *self.index.entry((pos.1, pos.0)).or_insert(next);
        if index == next {
            self.nodes.push(Node {
                f: 0,
                g: 0,
                pos,
                parent: None,
                open: false,
                closed: false,
            });
        }
        index
    }

    fn end_pos(&self) -> Pos {
        self.nodes[self.end as usize].pos
    }

    /// `OpenList::_compare` for the heap functions: `a->f > b->f`.
    fn heap_less(nodes: &[Node]) -> impl FnMut(&u32, &u32) -> bool + '_ {
        |&a, &b| nodes[a as usize].f > nodes[b as usize].f
    }

    fn push_open(&mut self, n: u32) {
        self.open.push(n);
        stdsort::push_heap(&mut self.open, Self::heap_less(&self.nodes));
    }

    fn pop_open(&mut self) -> Option<u32> {
        if self.open.is_empty() {
            return None;
        }
        stdsort::pop_heap(&mut self.open, Self::heap_less(&self.nodes));
        self.open.pop()
    }

    fn fixup_open(&mut self) {
        stdsort::make_heap(&mut self.open, Self::heap_less(&self.nodes));
    }

    /// `jumpP`: the next jump point from `p`, reached from `src`.
    fn jump_p(&mut self, p: Pos, src: Pos) -> Option<Pos> {
        let (dx, dy) = (p.0 - src.0, p.1 - src.1);
        if dx != 0 && dy != 0 {
            self.jump_d(p, dx, dy)
        } else if dx != 0 {
            self.jump_x(p, dx)
        } else {
            self.jump_y(p, dy)
        }
    }

    fn jump_d(&mut self, mut p: Pos, dx: i32, dy: i32) -> Option<Pos> {
        let end = self.end_pos();
        loop {
            if p == end {
                return Some(p);
            }
            let (x, y) = p;
            if (self.walkable(x - dx, y + dy) && !self.walkable(x - dx, y))
                || (self.walkable(x + dx, y - dy) && !self.walkable(x, y - dy))
            {
                return Some(p);
            }
            let gdx = self.walkable(x + dx, y);
            let gdy = self.walkable(x, y + dy);
            if gdx && self.jump_x((x + dx, y), dx).is_some() {
                return Some(p);
            }
            if gdy && self.jump_y((x, y + dy), dy).is_some() {
                return Some(p);
            }
            if (gdx || gdy) && self.walkable(x + dx, y + dy) {
                p = (x + dx, y + dy);
            } else {
                return None;
            }
        }
    }

    /// The walkability of the cells above and below `(x, y)`, as two bits.
    fn sides_y(&mut self, x: i32, y: i32) -> u32 {
        u32::from(self.walkable(x, y + 1)) | (u32::from(self.walkable(x, y - 1)) << 1)
    }

    fn sides_x(&mut self, x: i32, y: i32) -> u32 {
        u32::from(self.walkable(x + 1, y)) | (u32::from(self.walkable(x - 1, y)) << 1)
    }

    fn jump_x(&mut self, mut p: Pos, dx: i32) -> Option<Pos> {
        let y = p.1;
        let end = self.end_pos();
        let mut a = !self.sides_y(p.0, y);
        loop {
            let xx = p.0 + dx;
            let b = self.sides_y(xx, y);
            if (b & a) != 0 || p == end {
                return Some(p);
            }
            if !self.walkable(xx, y) {
                return None;
            }
            p.0 += dx;
            a = !b;
        }
    }

    fn jump_y(&mut self, mut p: Pos, dy: i32) -> Option<Pos> {
        let x = p.0;
        let end = self.end_pos();
        let mut a = !self.sides_x(x, p.1);
        loop {
            let yy = p.1 + dy;
            let b = self.sides_x(x, yy);
            if (a & b) != 0 || p == end {
                return Some(p);
            }
            if !self.walkable(x, yy) {
                return None;
            }
            p.1 += dy;
            a = !b;
        }
    }

    /// `findNeighbors`: the cells to jump from, in the order of C++.
    fn find_neighbors(&mut self, n: u32, out: &mut Vec<Pos>) {
        let (x, y) = self.nodes[n as usize].pos;
        let check = |s: &mut Self, out: &mut Vec<Pos>, dx: i32, dy: i32| {
            if s.walkable(x + dx, y + dy) {
                out.push((x + dx, y + dy));
            }
        };

        let Some(parent) = self.nodes[n as usize].parent else {
            // Straight moves, then diagonal ones with at least one of the two
            // tiles beside them walkable.
            for (dx, dy) in [(-1, 0), (0, -1), (0, 1), (1, 0)] {
                check(self, out, dx, dy);
            }
            for (dx, dy) in [(-1, -1), (-1, 1), (1, -1), (1, 1)] {
                if self.walkable(x + dx, y) || self.walkable(x, y + dy) {
                    check(self, out, dx, dy);
                }
            }
            return;
        };

        // The direction of the jump, each -1, 0 or 1.
        let (px, py) = self.nodes[parent as usize].pos;
        let dx = (x - px).signum();
        let dy = (y - py).signum();

        if dx != 0 && dy != 0 {
            // Natural neighbours.
            let walk_x = self.walkable(x + dx, y);
            if walk_x {
                out.push((x + dx, y));
            }
            let walk_y = self.walkable(x, y + dy);
            if walk_y {
                out.push((x, y + dy));
            }
            if walk_x || walk_y {
                check(self, out, dx, dy);
            }
            // Forced neighbours.
            if walk_y && !self.walkable(x - dx, y) {
                check(self, out, -dx, dy);
            }
            if walk_x && !self.walkable(x, y - dy) {
                check(self, out, dx, -dy);
            }
        } else if dx != 0 {
            if self.walkable(x + dx, y) {
                out.push((x + dx, y));
                if !self.walkable(x, y + 1) {
                    check(self, out, dx, 1);
                }
                if !self.walkable(x, y - 1) {
                    check(self, out, dx, -1);
                }
            }
        } else if dy != 0 && self.walkable(x, y + dy) {
            out.push((x, y + dy));
            if !self.walkable(x + 1, y) {
                check(self, out, 1, dy);
            }
            if !self.walkable(x - 1, y) {
                check(self, out, -1, dy);
            }
        }
    }

    /// `identifySuccessors`.
    fn identify_successors(&mut self, n: u32) {
        let mut neighbors = Vec::with_capacity(8);
        self.find_neighbors(n, &mut neighbors);
        let n_pos = self.nodes[n as usize].pos;
        for &cell in neighbors.iter().rev() {
            let Some(jp) = self.jump_p(cell, n_pos) else {
                continue;
            };
            let jn = self.node(jp);
            if self.nodes[jn as usize].closed {
                continue;
            }
            let extra_g = euclidean(jp, n_pos);
            let new_g = self.nodes[n as usize].g.wrapping_add(extra_g);
            let end = self.end_pos();
            let node = &mut self.nodes[jn as usize];
            if !node.open || new_g < node.g {
                node.g = new_g;
                node.f = node.g.wrapping_add(manhattan(jp, end));
                node.parent = Some(n);
                if !node.open {
                    node.open = true;
                    self.push_open(jn);
                } else {
                    self.fixup_open();
                }
            }
        }
    }

    /// `findPathGreedy`: diagonally, then straight, if nothing is in the way.
    fn find_path_greedy(&mut self, n: u32) -> bool {
        let (mut x, mut y) = self.nodes[n as usize].pos;
        let (ex, ey) = self.end_pos();
        let (adx, ady) = ((ex - x).abs(), (ey - y).abs());
        let dx = (ex - x).signum();
        let dy = (ey - y).signum();

        let mut mid = None;
        if x != ex && y != ey {
            let tx = x + dx * adx.min(ady);
            while x != tx {
                if self.walkable(x, y) && (self.walkable(x + dx, y) || self.walkable(x, y + dy)) {
                    x += dx;
                    y += dy;
                } else {
                    return false;
                }
            }
            if !self.walkable(x, y) {
                return false;
            }
            mid = Some((x, y));
        }

        while x != ex {
            x += dx;
            if !self.walkable(x, y) {
                return false;
            }
        }
        while y != ey {
            y += dy;
            if !self.walkable(x, y) {
                return false;
            }
        }

        match mid {
            Some(pos) => {
                let mid = self.node(pos);
                self.nodes[mid as usize].parent = Some(n);
                if mid != self.end {
                    self.nodes[self.end as usize].parent = Some(mid);
                }
            }
            None => self.nodes[self.end as usize].parent = Some(n),
        }
        true
    }

    /// `generatePath` with `step` 0: the way points back from the end, but for
    /// the start, reversed.
    fn generate_path(&self) -> Option<Vec<TilePosition>> {
        let mut next = &self.nodes[self.end as usize];
        next.parent?;
        let mut path = Vec::new();
        while let Some(parent) = next.parent {
            path.push(TilePosition::new(next.pos.0, next.pos.1));
            next = &self.nodes[parent as usize];
        }
        path.reverse();
        Some(path)
    }
}

/// `Heuristic::Manhattan`.
fn manhattan(a: Pos, b: Pos) -> u32 {
    ((a.0 - b.0).abs() + (a.1 - b.1).abs()) as u32
}

/// `Heuristic::Euclidean`, which takes `sqrtf`. The squares are integers below
/// 2^24, exact in `f32`; the root of an integer that is not a square is at least
/// 1/513 away from an integer here, so `f64` truncates to the same value.
fn euclidean(a: Pos, b: Pos) -> u32 {
    let (dx, dy) = (f64::from(a.0 - b.0), f64::from(a.1 - b.1));
    math::sqrt(dx * dx + dy * dy) as u32
}
