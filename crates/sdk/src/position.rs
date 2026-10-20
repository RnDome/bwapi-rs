//! What the positions of [`bwapi`] leave to the SDK: the Euclidean distance
//! needs a square root, and `core` has none.

use bwapi::{Position, TilePosition, WalkPosition};

use crate::math::sqrt;

/// The Euclidean distance between two positions of one scale.
pub trait PositionExt: Copy {
    /// The Euclidean distance, correctly rounded. BWAPI: `getDistance`.
    fn distance(self, other: Self) -> f64;
}

macro_rules! position_ext {
    ($($t:ty),*) => {$(
        impl PositionExt for $t {
            fn distance(self, other: Self) -> f64 {
                let (dx, dy) = (f64::from(self.x - other.x), f64::from(self.y - other.y));
                sqrt(dx * dx + dy * dy)
            }
        }
    )*};
}

position_ext!(Position, TilePosition, WalkPosition);

#[cfg(test)]
mod tests {
    use bwapi::Position;

    use super::PositionExt;

    #[test]
    fn distances() {
        let o = Position::new(0, 0);
        assert_eq!(o.distance(Position::new(3, 4)), 5.0);
        assert_eq!(o.distance(Position::new(-1, 1)), core::f64::consts::SQRT_2);
    }
}
