//! Positions in the three BWAPI scales.

macro_rules! position {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
        pub struct $name {
            pub x: i32,
            pub y: i32,
        }

        impl $name {
            pub const fn new(x: i32, y: i32) -> Self {
                $name { x, y }
            }
        }
    };
}

position!(
    /// A position in pixels. BWAPI: `BWAPI::Position`.
    Position
);
position!(
    /// A position in build tiles of 32×32 pixels. BWAPI: `BWAPI::TilePosition`.
    TilePosition
);
position!(
    /// A position in walk tiles of 8×8 pixels. BWAPI: `BWAPI::WalkPosition`.
    WalkPosition
);
