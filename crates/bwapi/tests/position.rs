//! Position arithmetic against the formulas of `BWAPI::Point`. No FFI is called.

use bwapi::{Position, TilePosition, WalkPosition};

#[test]
fn conversions_multiply_up_and_truncate_down_toward_zero() {
    assert_eq!(
        Position::from(TilePosition::new(3, -2)),
        Position::new(96, -64)
    );
    assert_eq!(
        WalkPosition::from(TilePosition::new(3, -2)),
        WalkPosition::new(12, -8)
    );
    assert_eq!(
        Position::from(WalkPosition::new(3, 1)),
        Position::new(24, 8)
    );
    assert_eq!(
        TilePosition::from(Position::new(63, 64)),
        TilePosition::new(1, 2)
    );
    assert_eq!(
        TilePosition::from(Position::new(-1, -33)),
        TilePosition::new(0, -1)
    );
    assert_eq!(
        WalkPosition::from(Position::new(15, -9)),
        WalkPosition::new(1, -1)
    );
    assert_eq!(
        TilePosition::from(WalkPosition::new(7, 8)),
        TilePosition::new(1, 2)
    );
}

#[test]
fn operators() {
    let p = Position::new(10, -4);
    assert_eq!(p + Position::new(1, 2), Position::new(11, -2));
    assert_eq!(p - Position::new(1, 2), Position::new(9, -6));
    assert_eq!(p * 3, Position::new(30, -12));
    assert_eq!(p / 3, Position::new(3, -1));
}

#[test]
fn division_by_zero_gives_the_bwapi_sentinel() {
    assert_eq!(Position::new(5, 5) / 0, Position::new(32000, 32000));
    assert_eq!(WalkPosition::new(5, 5) / 0, WalkPosition::new(4000, 4000));
    assert_eq!(TilePosition::new(5, 5) / 0, TilePosition::new(1000, 1000));
}

#[test]
fn distances() {
    let o = Position::new(0, 0);
    assert_eq!(o.distance_squared(Position::new(-3, 4)), 25);
    assert_eq!(o.chebyshev(Position::new(-7, 4)), 7);
}

#[test]
fn approx_distance_follows_the_engine_formula() {
    let o = Position::new(0, 0);
    // The short side is at most a quarter of the long one: the long side.
    assert_eq!(o.approx_distance(Position::new(100, 25)), 100);
    assert_eq!(o.approx_distance(Position::new(-25, -100)), 100);
    // 3*3>>3 = 1; 0 + 1 + 4 - 0 - 0.
    assert_eq!(o.approx_distance(Position::new(3, 4)), 5);
    // 3*100>>3 = 37; 1 + 37 + 100 - 6 - 1.
    assert_eq!(o.approx_distance(Position::new(100, 100)), 131);
}

#[test]
fn centers() {
    assert_eq!(TilePosition::new(2, 3).center(), Position::new(80, 112));
    assert_eq!(WalkPosition::new(2, 3).center(), Position::new(20, 28));
}

#[test]
fn order_is_by_x_then_y() {
    assert!(TilePosition::new(1, 9) < TilePosition::new(2, 0));
    assert!(TilePosition::new(1, 0) < TilePosition::new(1, 1));
}
