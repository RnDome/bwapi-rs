//! Positions in the three BWAPI scales, with the arithmetic of `BWAPI::Point`.

use core::ops::{Add, Div, Mul, Sub};

macro_rules! position {
    ($(#[$doc:meta])* $name:ident, $scale:literal) => {
        $(#[$doc])*
        ///
        /// Ordered by `x`, then `y`, as `BWAPI::Point::operator<`.
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name {
            pub x: i32,
            pub y: i32,
        }

        impl $name {
            /// Pixels in one unit of this scale.
            pub const SCALE: i32 = $scale;

            pub const fn new(x: i32, y: i32) -> Self {
                $name { x, y }
            }

            /// The integer approximation of the distance the engine uses.
            /// BWAPI: `getApproxDistance`, in its unsigned arithmetic.
            pub fn approx_distance(self, other: Self) -> i32 {
                let mut max = (self.x - other.x).unsigned_abs();
                let mut min = (self.y - other.y).unsigned_abs();
                if max < min {
                    core::mem::swap(&mut min, &mut max);
                }
                if min <= max >> 2 {
                    return max as i32;
                }
                let min_calc = min.wrapping_mul(3) >> 3;
                let sum = (min_calc >> 5).wrapping_add(min_calc).wrapping_add(max);
                sum.wrapping_sub(max >> 4).wrapping_sub(max >> 6) as i32
            }

            /// The larger of the two coordinate differences. BWEM: `queenWiseDist`.
            pub fn chebyshev(self, other: Self) -> i32 {
                (self.x - other.x).abs().max((self.y - other.y).abs())
            }

            /// The squared Euclidean distance, exact in integers.
            pub fn distance_squared(self, other: Self) -> i32 {
                let (dx, dy) = (self.x - other.x, self.y - other.y);
                dx * dx + dy * dy
            }
        }

        impl Add for $name {
            type Output = Self;

            fn add(self, rhs: Self) -> Self {
                $name::new(self.x + rhs.x, self.y + rhs.y)
            }
        }

        impl Sub for $name {
            type Output = Self;

            fn sub(self, rhs: Self) -> Self {
                $name::new(self.x - rhs.x, self.y - rhs.y)
            }
        }

        impl Mul<i32> for $name {
            type Output = Self;

            fn mul(self, rhs: i32) -> Self {
                $name::new(self.x * rhs, self.y * rhs)
            }
        }

        /// Truncates toward zero. Division by zero gives `(32000, 32000)` in pixels,
        /// as in BWAPI, instead of a panic.
        impl Div<i32> for $name {
            type Output = Self;

            fn div(self, rhs: i32) -> Self {
                if rhs == 0 {
                    return $name::new(32000 / $scale, 32000 / $scale);
                }
                $name::new(self.x.wrapping_div(rhs), self.y.wrapping_div(rhs))
            }
        }
    };
}

position!(
    /// A position in pixels. BWAPI: `BWAPI::Position`.
    Position, 1
);
position!(
    /// A position in build tiles of 32×32 pixels. BWAPI: `BWAPI::TilePosition`.
    TilePosition, 32
);
position!(
    /// A position in walk tiles of 8×8 pixels. BWAPI: `BWAPI::WalkPosition`.
    WalkPosition, 8
);

/// Between scales as BWAPI converts: multiplied up, divided down with truncation
/// toward zero.
macro_rules! convert {
    ($from:ident -> $to:ident) => {
        impl From<$from> for $to {
            fn from(p: $from) -> Self {
                if $from::SCALE > $to::SCALE {
                    let k = $from::SCALE / $to::SCALE;
                    $to::new(p.x * k, p.y * k)
                } else {
                    let k = $to::SCALE / $from::SCALE;
                    $to::new(p.x / k, p.y / k)
                }
            }
        }
    };
}

convert!(Position -> TilePosition);
convert!(Position -> WalkPosition);
convert!(TilePosition -> Position);
convert!(TilePosition -> WalkPosition);
convert!(WalkPosition -> Position);
convert!(WalkPosition -> TilePosition);

impl TilePosition {
    /// The center pixel of the tile.
    pub fn center(self) -> Position {
        Position::from(self) + Position::new(Self::SCALE / 2, Self::SCALE / 2)
    }
}

impl WalkPosition {
    /// The center pixel of the walk tile.
    pub fn center(self) -> Position {
        Position::from(self) + Position::new(Self::SCALE / 2, Self::SCALE / 2)
    }
}
