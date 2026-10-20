// Port of BWEM/include/bwapiExt.h and BWEM/include/utils.h of BWEM-community
// (https://github.com/N00byEdge/BWEM-community, 9a63141).
// Copyright (c) 2015, 2017, Igor Dimitrijevic. MIT/X11 License, see LICENSE-BWEM.

//! The geometric helpers BWEM uses internally.

use alloc::vec::Vec;

use bwapi::{Position, TilePosition, WalkPosition};

use crate::grid::GridPosition;
use crate::math::sqrt;

/// `utils::norm`: the square root of the integer squared norm.
pub(crate) fn norm(dx: i32, dy: i32) -> f64 {
    sqrt(f64::from(dx * dx + dy * dy))
}

/// `BWAPI_ext::dist`.
pub(crate) fn dist<P: GridPosition>(a: P, b: P) -> f64 {
    norm(a.x() - b.x(), a.y() - b.y())
}

/// `BWAPI_ext::roundedDist`: `int(0.5 + dist)`.
pub(crate) fn rounded_dist<P: GridPosition>(a: P, b: P) -> i32 {
    (0.5 + dist(a, b)) as i32
}

/// `BWAPI_ext::distToRectangle`: from `a` to the pixels of the tiles
/// `top_left..top_left + size`.
pub(crate) fn dist_to_rectangle(a: Position, top_left: TilePosition, size: TilePosition) -> i32 {
    let tl = Position::from(top_left);
    let br = Position::from(top_left + size) - Position::new(1, 1);
    if a.x >= tl.x {
        if a.x <= br.x {
            if a.y > br.y {
                a.y - br.y
            } else if a.y < tl.y {
                tl.y - a.y
            } else {
                0
            }
        } else if a.y > br.y {
            rounded_dist(a, br)
        } else if a.y < tl.y {
            rounded_dist(a, Position::new(br.x, tl.y))
        } else {
            a.x - br.x
        }
    } else if a.y > br.y {
        rounded_dist(a, Position::new(tl.x, br.y))
    } else if a.y < tl.y {
        rounded_dist(a, tl)
    } else {
        tl.x - a.x
    }
}

/// `BWAPI_ext::outerMiniTileBorder`: the mini tiles around the tiles
/// `top_left..top_left + size`, row by row.
pub(crate) fn outer_mini_tile_border(
    top_left: TilePosition,
    size: TilePosition,
) -> Vec<WalkPosition> {
    let top_left = WalkPosition::from(top_left) - WalkPosition::new(1, 1);
    let size = WalkPosition::from(size) + WalkPosition::new(2, 2);
    let mut border = Vec::new();
    for dy in 0..size.y {
        for dx in 0..size.x {
            if dy == 0 || dy == size.y - 1 || dx == 0 || dx == size.x - 1 {
                border.push(top_left + WalkPosition::new(dx, dy));
            }
        }
    }
    border
}

/// `utils::intersect`: whether the segments `a–b` and `c–d` intersect, in the
/// floating point arithmetic of the C++ code.
#[allow(clippy::too_many_arguments)]
pub(crate) fn intersect(
    ax: i32,
    ay: i32,
    bx: i32,
    by: i32,
    cx: i32,
    cy: i32,
    dx: i32,
    dy: i32,
) -> bool {
    let (p0_x, p0_y, p1_x, p1_y) = (f64::from(ax), f64::from(ay), f64::from(bx), f64::from(by));
    let (p2_x, p2_y, p3_x, p3_y) = (f64::from(cx), f64::from(cy), f64::from(dx), f64::from(dy));
    let (s1_x, s1_y) = (p1_x - p0_x, p1_y - p0_y);
    let (s2_x, s2_y) = (p3_x - p2_x, p3_y - p2_y);

    let s = (-s1_y * (p0_x - p2_x) + s1_x * (p0_y - p2_y)) / (-s2_x * s1_y + s1_x * s2_y);
    let t = (s2_x * (p0_y - p2_y) - s2_y * (p0_x - p2_x)) / (-s2_x * s1_y + s1_x * s2_y);

    (0.0..=1.0).contains(&s) && (0.0..=1.0).contains(&t)
}

/// `BWAPI_ext::makeBoundingBoxIncludePoint`.
pub(crate) fn include_point(
    top_left: &mut TilePosition,
    bottom_right: &mut TilePosition,
    a: TilePosition,
) {
    if a.x < top_left.x {
        top_left.x = a.x;
    }
    if a.x > bottom_right.x {
        bottom_right.x = a.x;
    }
    if a.y < top_left.y {
        top_left.y = a.y;
    }
    if a.y > bottom_right.y {
        bottom_right.y = a.y;
    }
}

/// `BWAPI_ext::makePointFitToBoundingBox`.
pub(crate) fn fit_to_box(a: &mut TilePosition, top_left: TilePosition, bottom_right: TilePosition) {
    if a.x < top_left.x {
        a.x = top_left.x;
    } else if a.x > bottom_right.x {
        a.x = bottom_right.x;
    }
    if a.y < top_left.y {
        a.y = top_left.y;
    } else if a.y > bottom_right.y {
        a.y = bottom_right.y;
    }
}
