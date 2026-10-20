// Port of BWEB (https://github.com/Cmccrave/BWEB, v1.17, d57980c), Block.cpp
// and Block.h.
// Copyright (c) 2017 Christian McCrave. MIT License, see LICENSE-BWEB.

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec;
use alloc::vec::Vec;

use bwapi::{Position, Race, TilePosition, UnitType};

use super::{Footprint, Layout, Station, Stations};
use crate::PositionExt;
use crate::bwem::{self, AreaId};
use crate::stdsort;

/// What the game knows at the start that the analysis of BWEB asks it: creep,
/// and `canBuildHere`, which also checks units and creep. A snapshot: BWEB asks
/// once, while it finds blocks and walls. `adapter::start_state` answers from
/// the game.
pub trait StartState {
    /// `Game::hasCreep`.
    fn has_creep(&self, tile: TilePosition) -> bool;
    /// `Game::canBuildHere` without a builder.
    fn can_build_here(&self, tile: TilePosition, footprint: &Footprint) -> bool;
}

/// A piece of a block: a building slot by size, or a step of the layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Piece {
    /// 2×2.
    Small,
    /// 3×2.
    Medium,
    /// 4×3.
    Large,
    /// 4×2.
    Wide,
    /// The 2×2 slot of an addon, one tile down; it counts as small.
    Addon,
    /// The next row of the layout.
    Row,
    /// A tile left empty.
    Space,
}

/// What a block was found for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BlockType {
    None,
    Start,
    Production,
    Proxy,
    Defensive,
    Supply,
    Tech,
}

/// A group of building slots that BWEB lays out together.
#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    tile: TilePosition,
    width: i32,
    height: i32,
    center: Position,
    small_tiles: BTreeSet<TilePosition>,
    medium_tiles: BTreeSet<TilePosition>,
    large_tiles: BTreeSet<TilePosition>,
    wide_tiles: BTreeSet<TilePosition>,
    kind: BlockType,
}

impl Block {
    fn new(
        tile: TilePosition,
        pieces: &[(TilePosition, Piece)],
        width: i32,
        height: i32,
        kind: BlockType,
    ) -> Block {
        let mut block = Block {
            tile,
            width,
            height,
            // `h / w`, not `h / 2`, and tiles added to pixels, as BWEB has it.
            center: Position::from(tile) + Position::new(width / 2, height / width),
            small_tiles: BTreeSet::new(),
            medium_tiles: BTreeSet::new(),
            large_tiles: BTreeSet::new(),
            wide_tiles: BTreeSet::new(),
            kind,
        };
        for &(placement, piece) in pieces {
            match piece {
                Piece::Small | Piece::Addon => block.small_tiles.insert(placement),
                Piece::Medium => block.medium_tiles.insert(placement),
                Piece::Large => block.large_tiles.insert(placement),
                Piece::Wide => block.wide_tiles.insert(placement),
                Piece::Row | Piece::Space => false,
            };
        }
        block
    }

    /// The top left tile of the block.
    pub fn tile_position(&self) -> TilePosition {
        self.tile
    }

    /// `getCenter`: the top left of the block, moved by half its width and by
    /// its height divided by its width, in pixels.
    pub fn center(&self) -> Position {
        self.center
    }

    /// The top left tiles of the 2×2 slots, addons among them.
    pub fn small_tiles(&self) -> &BTreeSet<TilePosition> {
        &self.small_tiles
    }

    /// The top left tiles of the 3×2 slots.
    pub fn medium_tiles(&self) -> &BTreeSet<TilePosition> {
        &self.medium_tiles
    }

    /// The top left tiles of the 4×3 slots.
    pub fn large_tiles(&self) -> &BTreeSet<TilePosition> {
        &self.large_tiles
    }

    /// The top left tiles of the 4×2 slots.
    pub fn wide_tiles(&self) -> &BTreeSet<TilePosition> {
        &self.wide_tiles
    }

    /// The slots for a building of the size of `footprint`.
    pub fn placements(&self, footprint: &Footprint) -> &BTreeSet<TilePosition> {
        match (footprint.tile_width, footprint.tile_height) {
            (4, 2) => &self.wide_tiles,
            (4, _) => &self.large_tiles,
            (3, _) => &self.medium_tiles,
            _ => &self.small_tiles,
        }
    }

    pub fn is_proxy(&self) -> bool {
        self.kind == BlockType::Proxy
    }

    pub fn is_defensive(&self) -> bool {
        self.kind == BlockType::Defensive
    }

    /// What the block was found for.
    pub fn block_type(&self) -> BlockType {
        self.kind
    }

    /// The width in tiles.
    pub fn width(&self) -> i32 {
        self.width
    }

    /// The height in tiles.
    pub fn height(&self) -> i32 {
        self.height
    }

    pub fn insert_small(&mut self, here: TilePosition) {
        self.small_tiles.insert(here);
    }

    pub fn insert_medium(&mut self, here: TilePosition) {
        self.medium_tiles.insert(here);
    }

    pub fn insert_large(&mut self, here: TilePosition) {
        self.large_tiles.insert(here);
    }

    pub fn insert_wide(&mut self, here: TilePosition) {
        self.wide_tiles.insert(here);
    }
}

/// Every block of the map: `BWEB::Blocks`.
#[derive(Clone, Debug, Default)]
pub struct Blocks {
    blocks: Vec<Block>,
}

/// The pieces of each kind in a list of pieces.
#[derive(Clone, Copy, Default)]
struct Counts {
    small: i32,
    medium: i32,
    large: i32,
    wide: i32,
}

impl Counts {
    fn of(pieces: &[Piece]) -> Counts {
        let count = |kind| pieces.iter().filter(|&&p| p == kind).count() as i32;
        Counts {
            small: count(Piece::Small),
            medium: count(Piece::Medium),
            large: count(Piece::Large),
            wide: count(Piece::Wide),
        }
    }
}

/// The state `findBlocks` keeps in globals while it finds the blocks.
struct Finder<'a, 'm> {
    bwem: &'m bwem::Map,
    layout: &'a mut Layout,
    stations: &'a Stations,
    race: Race,
    state: &'a dyn StartState,
    blocks: Vec<Block>,
    /// The pieces of every kind put in each area so far, by the area of the
    /// top left tile of the block; `None` for no area.
    pieces_per_area: BTreeMap<Option<AreaId>, BTreeMap<Piece, i32>>,
    /// The buildable tiles, column by column.
    available_tiles: Vec<TilePosition>,
}

impl Blocks {
    /// `Blocks::findBlocks`: the blocks of the race of the bot, `race`, in the
    /// starting main, then production and supply blocks in every main. Each
    /// block reserves its slots in `layout`, and the next avoids those
    /// reserves; so do the reserves of `stations` and of walls made before.
    /// `state` answers for creep and `canBuildHere` as of now.
    ///
    /// BWEB also fills a grid of block tiles that nothing reads; the port does
    /// not. Where BWEB dereferences the missing choke point of the starting
    /// main, the port finds no start blocks.
    pub fn find(
        bwem: &bwem::Map,
        layout: &mut Layout,
        stations: &Stations,
        race: Race,
        state: &dyn StartState,
    ) -> Blocks {
        let mut finder = Finder {
            bwem,
            layout,
            stations,
            race,
            state,
            blocks: Vec::new(),
            pieces_per_area: BTreeMap::new(),
            available_tiles: Vec::new(),
        };
        finder.initialize();
        finder.generate_tiles();
        finder.find_start_blocks();
        finder.find_production_blocks();
        finder.find_supply_blocks();
        Blocks {
            blocks: finder.blocks,
        }
    }

    pub fn blocks(&self) -> &[Block] {
        &self.blocks
    }

    /// The block whose middle tile is nearest to `here`.
    pub fn closest_block(&self, here: TilePosition) -> Option<&Block> {
        let mut dist_best = f64::MAX;
        let mut best = None;
        for block in &self.blocks {
            let tile = block.tile + TilePosition::new(block.width / 2, block.height / 2);
            let dist = here.distance(tile);
            if dist < dist_best {
                dist_best = dist;
                best = Some(block);
            }
        }
        best
    }

    /// Erases the first block that covers `here`. Its reserves stay, as in
    /// BWEB.
    pub fn erase_block(&mut self, here: TilePosition) {
        let covers = |b: &Block| {
            here.x >= b.tile.x
                && here.x < b.tile.x + b.width
                && here.y >= b.tile.y
                && here.y < b.tile.y + b.height
        };
        if let Some(i) = self.blocks.iter().position(covers) {
            self.blocks.remove(i);
        }
    }
}

/// `wherePieces`: the top left tile of each piece but rows and spaces, laid
/// out from `tile` row by row.
fn where_pieces(tile: TilePosition, pieces: &[Piece]) -> Vec<TilePosition> {
    let mut layout = Vec::new();
    let (mut row_height, mut row_width) = (0, 0);
    let (mut w, mut h) = (0, 0);
    let mut here = tile;
    for &p in pieces {
        let (advance, height) = match p {
            Piece::Small => (2, 2),
            Piece::Medium => (3, 2),
            Piece::Large => (4, 3),
            Piece::Wide => (4, 2),
            Piece::Addon => (2, 2),
            Piece::Space => (1, 2),
            Piece::Row => {
                w = w.max(row_width);
                h += row_height;
                row_width = 0;
                row_height = 0;
                here = tile + TilePosition::new(0, h);
                continue;
            }
        };
        match p {
            Piece::Addon => layout.push(here + TilePosition::new(0, 1)),
            Piece::Space => {}
            _ => layout.push(here),
        }
        here = here + TilePosition::new(advance, 0);
        row_width += advance;
        row_height = row_height.max(height);
    }
    layout
}

impl Finder<'_, '_> {
    /// `whichPieces`: the layout of a block `width` × `height` for the race,
    /// empty for a size it has none of.
    fn which_pieces(&self, width: i32, height: i32) -> Vec<Piece> {
        use Piece::{Addon, Large, Medium, Row, Small, Space, Wide};
        match self.race {
            Race::ZERG => match (height, width) {
                (2, 2) => vec![Small],
                (2, 3) => vec![Medium],
                (2, 4) => vec![Wide],
                (3, 4) => vec![Large],
                _ => Vec::new(),
            },
            Race::PROTOSS => match (height, width) {
                (2, 2) => vec![Small],
                (2, 5) => vec![Small, Medium],
                (4, 5) => vec![Small, Medium, Row, Small, Medium],
                (5, 4) => vec![Large, Row, Small, Small],
                (5, 8) => vec![Small, Medium, Medium, Row, Large, Large],
                (5, 16) => vec![
                    Small, Medium, Medium, Small, Medium, Medium, Row, Large, Large, Large, Large,
                ],
                (6, 10) => vec![Large, Addon, Large, Row, Large, Small, Large],
                (6, 18) => vec![
                    Large, Large, Addon, Large, Large, Row, Large, Large, Small, Large, Large,
                ],
                (8, 5) => vec![Large, Row, Small, Medium, Row, Large],
                (8, 4) => vec![Large, Row, Small, Small, Row, Large],
                (8, 8) => vec![
                    Large, Large, Row, Space, Space, Small, Small, Space, Space, Row, Large, Large,
                ],
                _ => Vec::new(),
            },
            Race::TERRAN => match (height, width) {
                (2, 3) => vec![Medium],
                (3, 6) => vec![Large, Addon],
                (6, 6) => vec![Large, Addon, Row, Large, Addon],
                (6, 10) => vec![Large, Large, Addon, Row, Large, Large, Addon],
                (9, 6) => vec![Large, Addon, Row, Large, Addon, Row, Large, Addon],
                (9, 10) => vec![
                    Large, Large, Addon, Row, Large, Large, Addon, Row, Large, Large, Addon,
                ],
                _ => Vec::new(),
            },
            _ => Vec::new(),
        }
    }

    fn count(&self, area: Option<AreaId>, piece: Piece) -> i32 {
        self.pieces_per_area
            .get(&area)
            .and_then(|counts| counts.get(&piece))
            .copied()
            .unwrap_or(0)
    }

    fn area(&self, tile: TilePosition) -> Option<AreaId> {
        self.bwem.area_at(tile).map(|a| a.id())
    }

    /// `initialize`: the main location and its surroundings reserved, 6×5.
    fn initialize(&mut self) {
        for station in self.stations.stations() {
            if !station.is_main() {
                continue;
            }
            if let Some(base) = station.base(self.bwem) {
                self.layout
                    .add_reserve(base.location() - TilePosition::new(1, 1), 6, 5);
            }
        }
    }

    /// `generateTiles`: every buildable tile, column by column.
    fn generate_tiles(&mut self) {
        let size = self.bwem.size();
        for x in 0..size.x {
            for y in 0..size.y {
                let t = TilePosition::new(x, y);
                if self.layout.buildable(t) {
                    self.available_tiles.push(t);
                }
            }
        }
    }

    /// `validTile`: a tile of the area of the station, of an empty area next to
    /// it, or right below a tile of its area.
    fn valid_tile(&self, t: TilePosition, station: &Station) -> bool {
        let Some(area) = self.bwem.area_at(t) else {
            return false;
        };
        let Some(base_area) = station.base(self.bwem).map(|b| b.area()) else {
            return false;
        };
        if area == base_area {
            return true;
        }
        if area.bases().next().is_none() && area.accessible_neighbours().any(|n| n == base_area) {
            return true;
        }
        let above = t - TilePosition::new(0, 1);
        self.bwem.valid(above) && self.bwem.area_at(above) == Some(base_area)
    }

    /// `canAddBlock`: a large piece of Zerg takes a hatchery; the tiles of the
    /// block, with a border of one but for Zerg and supply blocks, are in the
    /// map, buildable and not reserved, and for a supply block low; a supply
    /// block keeps 5 tiles from the choke point of the starting main.
    fn can_add_block(
        &self,
        here: TilePosition,
        width: i32,
        height: i32,
        pieces: &[(TilePosition, Piece)],
        kind: BlockType,
    ) -> bool {
        let hatchery = Footprint::of(UnitType::ZERG_HATCHERY).unwrap_or(Footprint::NONE);
        for &(tile, piece) in pieces {
            if piece == Piece::Large
                && self.race == Race::ZERG
                && !self.state.can_build_here(tile, &hatchery)
            {
                return false;
            }
        }

        let border = i32::from(
            (kind != BlockType::Supply && self.race != Race::ZERG) || kind == BlockType::Production,
        );
        for x in here.x - border..here.x + width + border {
            for y in here.y - border..here.y + height + border {
                let t = TilePosition::new(x, y);
                let Some(tile) = self.bwem.tile(t) else {
                    return false;
                };
                if kind == BlockType::Supply && tile.min_altitude() > 170 {
                    return false;
                }
                if !tile.is_buildable() || self.layout.is_reserved(t, 1, 1) {
                    return false;
                }
            }
        }

        if kind == BlockType::Supply {
            let center = Position::from(here) + Position::new(width * 32, height * 32);
            let choke = self
                .stations
                .starting_main()
                .and_then(|s| s.choke_point(self.bwem));
            if let Some(cp) = choke {
                if center.distance(Position::from(cp.center())) < 160.0 {
                    return false;
                }
            }
        }
        true
    }

    /// `createBlock`: the block, its pieces counted in its area, and its slots
    /// reserved; a large slot but of Zerg with a border of one.
    fn create_block(
        &mut self,
        here: TilePosition,
        pieces: &[(TilePosition, Piece)],
        width: i32,
        height: i32,
        kind: BlockType,
    ) {
        self.blocks
            .push(Block::new(here, pieces, width, height, kind));
        let area = self.area(here);
        for &(_, piece) in pieces {
            *self
                .pieces_per_area
                .entry(area)
                .or_default()
                .entry(piece)
                .or_default() += 1;
        }

        let zerg = self.race == Race::ZERG;
        for &(placement, piece) in pieces {
            match piece {
                Piece::Small => self.layout.add_reserve(placement, 2, 2),
                Piece::Medium => self.layout.add_reserve(placement, 3, 2),
                Piece::Large if zerg => self.layout.add_reserve(placement, 4, 3),
                Piece::Large => self
                    .layout
                    .add_reserve(placement - TilePosition::new(1, 1), 6, 5),
                Piece::Wide => self.layout.add_reserve(placement, 4, 2),
                Piece::Addon => self
                    .layout
                    .add_reserve(placement - TilePosition::new(1, 1), 4, 4),
                Piece::Row | Piece::Space => {}
            }
        }
    }

    /// `findBlock`: the first block, the largest first, at the first of
    /// `search_tiles` where `conditions` hold and it fits; Zerg blocks but
    /// production ones need creep at their corners. Whether one was made.
    fn find_block(
        &mut self,
        kind: BlockType,
        width: i32,
        height: i32,
        search_tiles: &[TilePosition],
        conditions: &dyn Fn(&Self, TilePosition, &Counts) -> bool,
    ) -> bool {
        let state = self.state;
        let creep_on_corners = |here: TilePosition, w: i32, h: i32| {
            [(0, 0), (w - 1, 0), (0, h - 1), (w - 1, h - 1)]
                .iter()
                .all(|&(dx, dy)| state.has_creep(here + TilePosition::new(dx, dy)))
        };
        for w in (2..=width).rev() {
            for h in (2..=height).rev() {
                // The pieces depend on the size only: none, no block of it.
                let pieces = self.which_pieces(w, h);
                let counts = Counts::of(&pieces);
                if pieces.is_empty() || where_pieces(TilePosition::new(0, 0), &pieces).is_empty() {
                    continue;
                }
                for &tile in search_tiles {
                    if !self.bwem.valid(tile) || !conditions(self, tile, &counts) {
                        continue;
                    }
                    if self.race == Race::ZERG
                        && kind != BlockType::Production
                        && !creep_on_corners(tile, w, h)
                    {
                        continue;
                    }
                    // `generatePieceLayout`: each piece but rows and spaces at its tile.
                    let layout = where_pieces(tile, &pieces);
                    let laid: Vec<(TilePosition, Piece)> = pieces
                        .iter()
                        .filter(|&&p| p != Piece::Row && p != Piece::Space)
                        .zip(layout)
                        .map(|(&p, t)| (t, p))
                        .collect();
                    if self.can_add_block(tile, w, h, &laid, kind) {
                        self.create_block(tile, &laid, w, h, kind);
                        return true;
                    }
                }
            }
        }
        false
    }

    /// `findStartBlocks`: by the starting main, Zerg as many 1-piece blocks as
    /// fit up to 1 small, 5 medium and 1 wide; Terran a defensive block nearest
    /// the choke point and a start block; Protoss two start blocks.
    fn find_start_blocks(&mut self) {
        let Some(main) = self.stations.starting_main() else {
            return;
        };
        let Some(base) = main.base(self.bwem) else {
            return;
        };
        let Some(choke) = main.choke_point(self.bwem) else {
            return;
        };
        let area = Some(base.area().id());
        let race = self.race;
        let start_ok = move |f: &Self, _: TilePosition, c: &Counts| match race {
            // Zerg needs 5 medium, 1 small, 1 wide.
            Race::ZERG => {
                c.medium + c.small + c.wide == 1
                    && !(f.count(area, Piece::Small) >= 1 && c.small > 0)
                    && !(f.count(area, Piece::Medium) >= 5 && c.medium > 0)
                    && !(f.count(area, Piece::Wide) >= 1 && c.wide > 0)
            }
            Race::PROTOSS => c.large >= 2 && c.medium >= 1,
            Race::TERRAN => c.large >= 1,
            _ => true,
        };
        let def_ok = |_: &Self, _: TilePosition, _: &Counts| true;

        let main_tile = base.location();
        let choke_tile = TilePosition::from(choke.center());
        let mut tiles_dist_start = self.available_tiles.clone();
        let mut tiles_dist_choke = self.available_tiles.clone();
        stdsort::sort(&mut tiles_dist_start, |a, b| {
            a.distance(main_tile) < b.distance(main_tile)
        });
        stdsort::sort(&mut tiles_dist_choke, |a, b| {
            a.distance(choke_tile) < b.distance(choke_tile)
        });

        match race {
            Race::ZERG => {
                while self.find_block(BlockType::Start, 4, 3, &tiles_dist_start, &start_ok) {}
            }
            Race::TERRAN => {
                self.find_block(BlockType::Defensive, 3, 2, &tiles_dist_choke, &def_ok);
                self.find_block(BlockType::Start, 10, 6, &tiles_dist_start, &start_ok);
            }
            _ => {
                self.find_block(BlockType::Start, 8, 5, &tiles_dist_start, &start_ok);
                self.find_block(BlockType::Start, 8, 5, &tiles_dist_start, &start_ok);
            }
        }
    }

    /// The available tiles of the area of a main station, by `less`.
    fn tiles_of(
        &self,
        station: &Station,
        less: impl FnMut(&TilePosition, &TilePosition) -> bool,
    ) -> Vec<TilePosition> {
        let mut tiles: Vec<TilePosition> = self
            .available_tiles
            .iter()
            .copied()
            .filter(|&t| self.valid_tile(t, station))
            .collect();
        stdsort::sort(&mut tiles, less);
        tiles
    }

    /// `findProductionBlocks`: in each main with a choke point, blocks nearest
    /// the choke point (Zerg: halfway to the main), with caps of pieces per
    /// area: Zerg solo hatcheries, up to 6.
    fn find_production_blocks(&mut self) {
        let race = self.race;
        let production_ok = move |f: &Self, tile: TilePosition, c: &Counts| {
            let area = f.area(tile);
            let have = |piece| f.count(area, piece);
            match race {
                Race::PROTOSS => {
                    !((c.large > 0 && have(Piece::Large) >= 16 && have(Piece::Medium) < 8)
                        || (c.medium > 0 && have(Piece::Medium) >= 12)
                        || (c.small > 0 && c.medium == 0 && c.large == 0)
                        || (c.large > 0 && have(Piece::Large) >= 12))
                }
                Race::ZERG => {
                    !(c.medium + c.small + c.wide > 0 || have(Piece::Large) + c.large >= 6)
                }
                Race::TERRAN => !(c.medium > 0 || (c.large > 0 && have(Piece::Large) >= 12)),
                _ => true,
            }
        };

        let stations = self.stations;
        for station in stations.stations() {
            let (Some(base), Some(choke)) =
                (station.base(self.bwem), station.choke_point(self.bwem))
            else {
                continue;
            };
            if !station.is_main() {
                continue;
            }
            let mut desired = TilePosition::from(choke.center());
            if race == Race::ZERG {
                desired = (desired + base.location()) / 2;
            }
            let tiles = self.tiles_of(station, |a, b| a.distance(desired) < b.distance(desired));
            while self.find_block(BlockType::Production, 20, 20, &tiles, &production_ok) {}
        }
    }

    /// `findSupplyBlocks`: but for Zerg, whose supply flies, in each main with
    /// a choke point, supply depots or pylons farthest from its choke point and
    /// the center of the map, up to 25 depots or 16 pylons per area.
    fn find_supply_blocks(&mut self) {
        if self.race == Race::ZERG {
            return;
        }
        let supply_width = if self.race == Race::PROTOSS { 2 } else { 3 };
        let supply_ok = move |f: &Self, tile: TilePosition, _: &Counts| {
            let area = f.area(tile);
            !((supply_width == 2 && f.count(area, Piece::Small) >= 16)
                || (supply_width == 3 && f.count(area, Piece::Medium) >= 25))
        };

        let center = TilePosition::from(self.bwem.center());
        let stations = self.stations;
        for station in stations.stations() {
            let Some(choke) = station.choke_point(self.bwem) else {
                continue;
            };
            if !station.is_main() {
                continue;
            }
            let desired = TilePosition::from(choke.center());
            let far = |t: &TilePosition| t.distance(desired) + t.distance(center);
            let tiles = self.tiles_of(station, |a, b| far(a) > far(b));
            while self.find_block(BlockType::Supply, supply_width, 2, &tiles, &supply_ok) {}
        }
    }
}
