/*!
# Examples

```
extern crate bwapi;
use bwapi::position::*;

let pos = Position::new(640, 480);

let mulpl = &pos * 2;
assert!(mulpl.x() == 640 * 2);
assert!(mulpl.y() == 480 * 2);

```

```
extern crate bwapi;
extern crate bwapi_sys;

use bwapi::position::*;

let pos = Position::new(640, 480);
let walk_pos = WalkPosition::from(pos);
// or #![feature(type_ascription)]
//    let walk_pos = pos.into() : WalkPosition;
let sys_walk_pos: bwapi_sys::WalkPosition = walk_pos.into();
```
*/

use bwapi_sys as sys;

use std::ops::Add;
use std::ops::Sub;
use std::ops::Mul;
use std::ops::Div;

pub trait Point {
    fn x(&self) -> isize;
    fn y(&self) -> isize;
    fn scale()  -> isize;

    /// The length of this point from (0,0)
    fn length(&self) -> f64 {
        let x = self.x().pow(2);
        let y = self.y().pow(2);
        ((x + y) as f64).sqrt()
    }
}

macro_rules! create_point_type {
    ($(#[$attr_struct:meta])* struct $name:ident $(#[$attr_scale:meta])* scale: $scale:expr) => {
        $(#[$attr_struct])*
        #[derive(Eq, PartialEq, Debug)]
        pub struct $name {
            x: isize,
            y: isize
        }
        impl $name {
            pub fn new(x: isize, y: isize) -> $name {
                Self { x, y }
            }
            /// The distance between this point and position.
            pub fn distance<'a>(&'a self, position: &'a Self) -> f64 {
                (self - position).length()
            }
        }
        impl Point for $name {
            fn x(&self) -> isize { self.x }
            fn y(&self) -> isize { self.y }
            $(#[$attr_scale])*
            fn scale()  -> isize { $scale }
        }
        impl From<sys::$name> for $name {
            fn from(sys_pos: sys::$name) -> $name {
                Self { x: sys_pos.x as isize, y: sys_pos.y as isize }
            }
        }
        impl From<$name> for sys::$name {
            fn from(pos: $name) -> sys::$name {
                Self { x: pos.x as i32, y: pos.y as i32 }
            }
        }
    };
}

create_point_type!(
    /// Indicates a position that is `1×1` pixel in size. This is the most precise position type.
    struct Position
    /// The scale of a [`Position`](#struct.Position). Each position corresponds to a 1x1 pixel area.
    scale: 1
);
create_point_type!(
    /// Indicates a position that is `8×8` pixels in size.
    struct WalkPosition
    /// The scale of a [`WalkPosition`](#struct.WalkPosition). Each walk position corresponds to an 8x8 pixel area.
    scale: 8
);
create_point_type!(
    /// Indicates a position that is `32×32` pixels in size.
    struct TilePosition
    /// The scale of a [`TilePosition`](#struct.TilePosition). Each tile position corresponds to a 32x32 pixel area.
    scale: 32
);

macro_rules! create_position_cast {
    ( $bigger:ty => $smaller:ty ) => {
        impl From<$bigger> for $smaller {
            fn from(pos: $bigger) -> $smaller {
                assert!(<$bigger>::scale() > <$smaller>::scale());
                let scale = <$bigger>::scale() / <$smaller>::scale();
                Self { x: pos.x() * scale, y: pos.y() * scale }
            }
        }
        impl From<$smaller> for $bigger {
            fn from(pos: $smaller) -> $bigger {
                assert!(<$bigger>::scale() > <$smaller>::scale());
                let scale = <$bigger>::scale() / <$smaller>::scale();
                Self { x: pos.x() / scale, y: pos.y() / scale }
            }
        }
    };
}

create_position_cast!(WalkPosition => Position);
create_position_cast!(TilePosition => Position);
create_position_cast!(TilePosition => WalkPosition);

macro_rules! create_point_ops {
    ( binary: $trait:ident method: $method:ident as $op:tt for $type:ty ) => {
        /// T op T
        impl $trait<$type> for $type {
            type Output = $type;
            fn $method(self, other: $type) -> $type {
                Self::Output { x: self.x $op other.x, y: self.y $op other.y }
            }
        }
        /// &T op T
        impl<'a> $trait<$type> for &'a $type {
            type Output = $type;
            fn $method(self, other: $type) -> $type {
                Self::Output { x: self.x $op other.x, y: self.y $op other.y }
            }
        }
        /// T op &T
        impl<'a> $trait<&'a $type> for $type {
            type Output = $type;
            fn $method(self, other: &'a $type) -> $type {
                Self::Output { x: self.x $op other.x, y: self.y $op other.y }
            }
        }
        /// &T op &T
        impl<'a, 'b> $trait<&'a $type> for &'b $type {
            type Output = $type;
            fn $method(self, other: &'a $type) -> $type {
                Self::Output { x: self.x $op other.x, y: self.y $op other.y }
            }
        }
    };
    ( scalar: $trait:ident method: $method:ident as $op:tt for $type:ty, $other:ty ) => {
        /// T op x
        impl $trait<$other> for $type {
            type Output = $type;
            fn $method(self, other: $other) -> $type {
                Self::Output { x: self.x $op other, y: self.y $op other }
            }
        }
        /// &T op x
        impl<'a> $trait<$other> for &'a $type {
            type Output = $type;
            fn $method(self, other: $other) -> $type {
                Self::Output { x: self.x $op other, y: self.y $op other }
            }
        }
        /// T op &x
        impl<'a> $trait<&'a $other> for $type {
            type Output = $type;
            fn $method(self, other: &'a $other) -> $type {
                Self::Output { x: self.x $op other, y: self.y $op other }
            }
        }
        /// &T op &x
        impl<'a, 'b> $trait<&'a $other> for &'b $type {
            type Output = $type;
            fn $method(self, other: &'a $other) -> $type {
                Self::Output { x: self.x $op other, y: self.y $op other }
            }
        }
    };
    ( $name:ty ) => {
        create_point_ops!(binary: Add method: add as + for $name );
        create_point_ops!(binary: Sub method: sub as - for $name );
        create_point_ops!(scalar: Mul method: mul as * for $name, isize);
        create_point_ops!(scalar: Div method: div as / for $name, isize);
    };
}

create_point_ops!(Position);
create_point_ops!(WalkPosition);
create_point_ops!(TilePosition);

#[cfg(test)]
mod tests {
    use bwapi_sys as sys;
    use super::*;

    #[test]
    fn test_position_distance() {
        let p1 = Position::new(1, 2);
        let p2 = Position::new(1, 2);
        assert!(p2.distance(&p1) < 0.001);
    }

    #[test]
    fn position_add() {
        let p1 = Position::new(1, 5) + Position::new(2, 4);
        assert_eq!(p1, Position::new(3, 9));
        let p2 = &p1 + Position::new(1, 2);
        assert_eq!(p2, Position::new(4, 11));
        let p3 = p2 + &p1;
        assert_eq!(p3, Position::new(7, 20));
        let p4 = &p3 + &p1;
        assert_eq!(p4, Position::new(10, 29));
    }

    #[test]
    fn position_mul() {
        let p1 = Position::new(1, 2) * 3;
        assert_eq!(p1, Position::new(3, 6));
        let p2 = &p1 * 3;
        assert_eq!(p2, Position::new(9, 18));
        let p3 = p2 * &3;
        assert_eq!(p3, Position::new(27, 54));
        let p4 = &p3 * &2;
        assert_eq!(p4, Position::new(54, 108));
    }

    #[test]
    fn walkposition_from_position() {
        let p1 = Position::new(8, 16);
        let p2: WalkPosition = From::from(p1);
        assert_eq!(p2, WalkPosition::new(1, 2));

        let p3 = Position::new(8, 16);
        let p4: WalkPosition = p3.into();
        assert_eq!(p4, WalkPosition::new(1, 2));
    }

    #[test]
    fn position_from_walkposition() {
        let p1 = WalkPosition::new(1, 2);
        let p2: Position = From::from(p1);
        assert_eq!(p2, Position::new(8, 16));

        let p3 = WalkPosition::new(1, 2);
        let p4: Position = p3.into();
        assert_eq!(p4, Position::new(8, 16));
    }

    #[test]
    fn position_cast_sys() {
        let p1 = Position::new(32, 16);
        let sys_p1: sys::Position = From::from(p1);
        let p2: Position = sys_p1.into();
        let p3: WalkPosition = p2.into();
        assert_eq!(p3, WalkPosition::new(4, 2));
    }
}
