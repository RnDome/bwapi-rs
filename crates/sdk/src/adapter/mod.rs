//! The only module that reads the game: it turns what BWAPI reports into the
//! data the algorithms take.

mod fap;

pub use fap::{FapUpgrades, fap_unit, fap_unit_of_type};

use bwapi::{Context, Position, TilePosition, UnitType, WalkPosition};

use crate::bweb::{Footprint, StartState};
use crate::bwem::{MapInput, NeutralInput, NeutralKind};
use crate::grid::Grid;

/// What BWEB reads of the unit type.
pub fn footprint(ty: UnitType) -> Footprint {
    Footprint {
        type_id: ty.id(),
        tile_width: ty.tile_width(),
        tile_height: ty.tile_height(),
        width: ty.width(),
        height: ty.height(),
        requires_creep: ty.requires_creep(),
        is_resource_depot: ty.is_resource_depot(),
    }
}

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

    fn can_build_here(&self, tile: TilePosition, footprint: &Footprint) -> bool {
        self.0
            .game()
            .can_build_here(tile, UnitType::from_id(footprint.type_id))
    }
}

/// What BWAPI reports of a static neutral unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StaticNeutral {
    pub unit_id: i32,
    pub unit_type: UnitType,
    pub initial_type: UnitType,
    pub initial_tile_position: TilePosition,
    pub initial_position: Position,
    pub initial_resources: i32,
}

/// A static neutral classified as BWEM's `InitializeNeutrals` does: buildings
/// are minerals, geysers or static buildings; of the other units pit doors are
/// static buildings, and eggs, critters and anything else are
/// [`NeutralKind::Other`], which only BWEB looks at.
pub fn neutral_input(neutral: &StaticNeutral) -> NeutralInput {
    let ty = neutral.unit_type;
    let kind = if ty.is_building() {
        if ty.is_mineral_field() {
            NeutralKind::Mineral
        } else if ty == UnitType::RESOURCE_VESPENE_GEYSER {
            NeutralKind::Geyser
        } else {
            NeutralKind::StaticBuilding
        }
    } else if ty == UnitType::SPECIAL_PIT_DOOR || ty == UnitType::SPECIAL_RIGHT_PIT_DOOR {
        NeutralKind::StaticBuilding
    } else {
        NeutralKind::Other
    };

    let tile_position = neutral.initial_tile_position;
    let mut top_left = tile_position;
    if ty == UnitType::SPECIAL_RIGHT_PIT_DOOR {
        top_left.x += 1;
    }
    NeutralInput {
        unit_id: neutral.unit_id,
        type_id: ty.id(),
        kind,
        top_left,
        size: neutral.initial_type.tile_size(),
        pos: neutral.initial_position,
        initial_amount: neutral.initial_resources,
        tile_position,
        building: ty.is_building(),
        immobile: ty.top_speed() == 0.0,
    }
}

/// What BWEM's `Map::Initialize` and BWEB's `Map::onStart` read from the game,
/// with the static neutrals by [`neutral_input`]. At the start of a game they
/// are the neutral units BWEB reads.
pub fn map_input(cx: Context<'_, '_>) -> MapInput {
    let game = cx.game();
    let (width, height) = (game.map_width(), game.map_height());

    let neutrals = game
        .static_neutral_units()
        .iter(cx)
        .map(|unit| {
            neutral_input(&StaticNeutral {
                unit_id: unit.id(),
                unit_type: unit.unit_type(),
                initial_type: unit.initial_type(),
                initial_tile_position: unit.initial_tile_position(),
                initial_position: unit.initial_position(),
                initial_resources: unit.initial_resources(),
            })
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
