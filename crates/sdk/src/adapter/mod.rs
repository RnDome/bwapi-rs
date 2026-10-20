//! The only module that reads the game: it turns what BWAPI reports into the
//! data the algorithms take.

mod fap;

pub use fap::fap_unit;

use bwapi::{Context, TilePosition, UnitType, WalkPosition};

use crate::bweb::StartState;
use crate::bwem::{MapInput, NeutralInput};
use crate::grid::Grid;

/// What the game knows now of creep and of where buildings fit, for the
/// analysis of BWEB's blocks and walls.
pub fn start_state<'a, 'game: 'a>(cx: Context<'a, 'game>) -> impl StartState + 'a {
    GameState(cx)
}

struct GameState<'a, 'game>(Context<'a, 'game>);

impl StartState for GameState<'_, '_> {
    fn has_creep(&self, tile: TilePosition) -> bool {
        self.0.game().has_creep(tile)
    }

    fn can_build_here(&self, tile: TilePosition, ty: UnitType) -> bool {
        self.0.game().can_build_here(tile, ty)
    }
}

/// What BWEM's `Map::Initialize` and BWEB's `Map::onStart` read from the game.
/// At the start of a game the static neutral units are the neutral units BWEB
/// reads.
pub fn map_input(cx: Context<'_, '_>) -> MapInput {
    let game = cx.game();
    let (width, height) = (game.map_width(), game.map_height());

    let neutrals = game
        .static_neutral_units()
        .iter(cx)
        .map(|unit| NeutralInput {
            unit_id: unit.id(),
            unit_type: unit.unit_type(),
            initial_type: unit.initial_type(),
            initial_tile_position: unit.initial_tile_position(),
            initial_position: unit.initial_position(),
            initial_resources: unit.initial_resources(),
        })
        .collect();

    MapInput {
        walkable: Grid::from_fn(width * 4, height * 4, |w: WalkPosition| game.is_walkable(w)),
        buildable: Grid::from_fn(width, height, |t: TilePosition| game.is_buildable(t)),
        ground_height: Grid::from_fn(width, height, |t: TilePosition| {
            game.ground_height(t).clamp(0, 255) as u8
        }),
        neutrals,
        start_locations: game.start_locations(),
    }
}
