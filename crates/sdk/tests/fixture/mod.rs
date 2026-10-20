//! The fixtures of the tests: what BWAPI reports of a map at the start of a
//! game, as each race, so that the tests need neither the game nor its MPQs.
//! `openbw-host` writes them (`UPDATE_FIXTURES=1`, test `fixtures`) and checks
//! that [`Fixture::map_input`] is what `adapter::map_input` reads from the game.
//!
//! The file follows BWAPI, not [`MapInput`]: a change of the input of the SDK
//! changes [`Fixture::map_input`] only. Line by line:
//!
//! ```text
//! fixture 1
//! map scai/(2)Showdown.scx              the name under maps/
//! hash <map hash>                        Game::mapHash
//! size 64 192                            in tiles
//! start_locations 7,10 51,174
//! walkable 256 768                       a grid: width, height, then its rows
//! buildable 64 192
//! ground_height 64 192
//! start 1 7,10 enemy -                   as race 1: the start location and the
//!                                          race of the enemy, `-` for none
//! neutrals 2                             then one line each, in the order of
//! 179 176 176 12,3 400,112 1500            getStaticNeutralUnits: id, type,
//! 180 188 188 30,4 1024,144 5000           initial type, initial tile, initial
//!                                          position, initial resources
//! creep 64 192                           hasCreep
//! can_build_here 106 64 192              canBuildHere of the resource depot of
//!                                          the race, type 106
//! ```
//!
//! A row is runs of values, `v` or `v*n`; `kx` before it repeats the row `k`
//! times. The neutrals belong to the race: the units of the start are made
//! before them, so their ids and their order depend on it.

#![allow(dead_code)]

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use bwapi::{Position, Race, TilePosition, UnitType, WalkPosition};
use bwapi_sdk::adapter::{StaticNeutral, neutral_input};
use bwapi_sdk::bweb::{Footprint, StartState};
use bwapi_sdk::bwem::MapInput;
use bwapi_sdk::grid::{Grid, GridPosition};

/// The version of the format, on the first line.
const VERSION: i32 = 1;

/// What BWAPI reports of a map at the start of a game.
#[derive(Clone, Debug, PartialEq)]
pub struct Fixture {
    /// The name under `maps/`, such as `scai/(2)Showdown.scx`.
    pub map: String,
    pub hash: String,
    /// In tiles.
    pub size: (i32, i32),
    pub start_locations: Vec<TilePosition>,
    pub walkable: Grid<WalkPosition, bool>,
    pub buildable: Grid<TilePosition, bool>,
    pub ground_height: Grid<TilePosition, i32>,
    /// One per race the bot played.
    pub starts: Vec<Start>,
}

/// What the game reports at the start for the race the bot plays.
#[derive(Clone, Debug, PartialEq)]
pub struct Start {
    pub race: Race,
    pub start_location: TilePosition,
    pub enemy: Option<Race>,
    pub neutrals: Vec<StaticNeutral>,
    pub creep: Grid<TilePosition, bool>,
    /// `canBuildHere` of the resource depot of the race, the only type BWEB
    /// asks it of.
    pub depot: Grid<TilePosition, bool>,
}

/// The file of the fixture of `map`, a name under `maps/`, in `data`, the
/// directory of the data of the tests.
pub fn path(data: &Path, map: &str) -> PathBuf {
    data.join(Path::new(map).with_extension(""))
        .join("input.txt")
}

impl Fixture {
    /// What BWEM and BWEB take, with the neutrals of `start`.
    pub fn map_input(&self, start: &Start) -> MapInput {
        let ground_height = &self.ground_height;
        MapInput {
            walkable: self.walkable.clone(),
            buildable: self.buildable.clone(),
            ground_height: Grid::from_fn(self.size.0, self.size.1, |t: TilePosition| {
                ground_height[t].clamp(0, i32::from(u8::MAX)) as u8
            }),
            neutrals: start.neutrals.iter().map(neutral_input).collect(),
            start_locations: self.start_locations.clone(),
        }
    }

    /// The start as `race`.
    pub fn start(&self, race: Race) -> Option<&Start> {
        self.starts.iter().find(|s| s.race == race)
    }

    pub fn to_text(&self) -> String {
        let mut out = String::new();
        let tile = |t: TilePosition| format!("{},{}", t.x, t.y);
        writeln!(out, "fixture {VERSION}").unwrap();
        writeln!(out, "map {}", self.map).unwrap();
        writeln!(out, "hash {}", self.hash).unwrap();
        writeln!(out, "size {} {}", self.size.0, self.size.1).unwrap();
        let starts: Vec<String> = self.start_locations.iter().map(|&t| tile(t)).collect();
        writeln!(out, "start_locations {}", starts.join(" ")).unwrap();
        write_grid(&mut out, "walkable", &self.walkable, |&b| i32::from(b));
        write_grid(&mut out, "buildable", &self.buildable, |&b| i32::from(b));
        write_grid(&mut out, "ground_height", &self.ground_height, |&h| h);
        for s in &self.starts {
            let enemy = s.enemy.map_or("-".to_owned(), |r| r.id().to_string());
            writeln!(
                out,
                "start {} {} enemy {enemy}",
                s.race.id(),
                tile(s.start_location)
            )
            .unwrap();
            writeln!(out, "neutrals {}", s.neutrals.len()).unwrap();
            for n in &s.neutrals {
                let p = n.initial_position;
                writeln!(
                    out,
                    "{} {} {} {} {},{} {}",
                    n.unit_id,
                    n.unit_type.id(),
                    n.initial_type.id(),
                    tile(n.initial_tile_position),
                    p.x,
                    p.y,
                    n.initial_resources
                )
                .unwrap();
            }
            write_grid(&mut out, "creep", &s.creep, |&b| i32::from(b));
            let name = format!("can_build_here {}", s.race.resource_depot().id());
            write_grid(&mut out, &name, &s.depot, |&b| i32::from(b));
        }
        out
    }

    pub fn parse(text: &str) -> Result<Fixture, String> {
        let mut lines = Lines(text.lines().enumerate());
        let version = lines.field("fixture")?;
        if version != VERSION.to_string() {
            return Err(format!("format {version}, not {VERSION}"));
        }
        let map = lines.field("map")?.to_owned();
        let hash = lines.field("hash")?.to_owned();
        let size = pair(lines.field("size")?, ' ')?;
        let start_locations = lines
            .field("start_locations")?
            .split_whitespace()
            .map(tile)
            .collect::<Result<_, _>>()?;
        let walkable = lines.grid("walkable", |v| Ok(v != 0))?;
        let buildable = lines.grid("buildable", |v| Ok(v != 0))?;
        let ground_height = lines.grid("ground_height", Ok)?;
        let mut starts = Vec::new();
        while let Some((i, line)) = lines.0.next() {
            let f: Vec<&str> = line.split_whitespace().collect();
            let ["start", race, at, "enemy", enemy] = f[..] else {
                return Err(format!("line {}: not a start", i + 1));
            };
            let count: usize = number(lines.field("neutrals")?)?;
            let mut neutrals = Vec::with_capacity(count);
            for _ in 0..count {
                let (n, line) = lines.next()?;
                let f: Vec<&str> = line.split_whitespace().collect();
                let [id, ty, initial, tile_position, position, resources] = f[..] else {
                    return Err(format!("line {n}: not a neutral"));
                };
                let (x, y) = pair(position, ',')?;
                neutrals.push(StaticNeutral {
                    unit_id: number(id)?,
                    unit_type: UnitType::from_id(number(ty)?),
                    initial_type: UnitType::from_id(number(initial)?),
                    initial_tile_position: tile(tile_position)?,
                    initial_position: Position::new(x, y),
                    initial_resources: number(resources)?,
                });
            }
            let race = Race::from_id(number(race)?);
            let creep = lines.grid("creep", |v| Ok(v != 0))?;
            let name = format!("can_build_here {}", race.resource_depot().id());
            let depot = lines.grid(&name, |v| Ok(v != 0))?;
            starts.push(Start {
                race,
                start_location: tile(at)?,
                enemy: match enemy {
                    "-" => None,
                    r => Some(Race::from_id(number(r)?)),
                },
                neutrals,
                creep,
                depot,
            });
        }
        Ok(Fixture {
            map,
            hash,
            size,
            start_locations,
            walkable,
            buildable,
            ground_height,
            starts,
        })
    }
}

impl StartState for Start {
    fn has_creep(&self, tile: TilePosition) -> bool {
        self.creep.get(tile).copied().unwrap_or(false)
    }

    fn can_build_here(&self, tile: TilePosition, footprint: &Footprint) -> bool {
        assert_eq!(
            footprint.type_id,
            self.race.resource_depot().id(),
            "the fixture holds canBuildHere of the resource depot of the race only"
        );
        self.depot.get(tile).copied().unwrap_or(false)
    }
}

/// `name width height`, then the rows of `grid`.
pub fn write_grid<P: GridPosition, T>(
    out: &mut String,
    name: &str,
    grid: &Grid<P, T>,
    value: impl Fn(&T) -> i32,
) {
    let width = grid.width().max(1) as usize;
    writeln!(out, "{name} {} {}", grid.width(), grid.height()).unwrap();
    let rows: Vec<String> = grid
        .cells()
        .chunks(width)
        .map(|row| {
            let mut runs = Vec::new();
            let mut i = 0;
            while i < row.len() {
                let v = value(&row[i]);
                let n = row[i..].iter().take_while(|c| value(c) == v).count();
                runs.push(if n == 1 {
                    v.to_string()
                } else {
                    format!("{v}*{n}")
                });
                i += n;
            }
            runs.join(" ")
        })
        .collect();
    let mut i = 0;
    while i < rows.len() {
        let k = rows[i..].iter().take_while(|r| **r == rows[i]).count();
        if k == 1 {
            writeln!(out, "{}", rows[i]).unwrap();
        } else {
            writeln!(out, "{k}x {}", rows[i]).unwrap();
        }
        i += k;
    }
}

struct Lines<'a>(std::iter::Enumerate<std::str::Lines<'a>>);

impl<'a> Lines<'a> {
    /// The next line, numbered from 1.
    fn next(&mut self) -> Result<(usize, &'a str), String> {
        self.0
            .next()
            .map(|(i, l)| (i + 1, l))
            .ok_or_else(|| "the file ends too early".to_owned())
    }

    /// The rest of the next line, which starts with `key`.
    fn field(&mut self, key: &str) -> Result<&'a str, String> {
        let (n, line) = self.next()?;
        line.strip_prefix(key)
            .and_then(|rest| rest.strip_prefix(' '))
            .ok_or_else(|| format!("line {n}: not `{key}`"))
    }

    /// A grid written by [`write_grid`] as `name`.
    fn grid<P: GridPosition, T>(
        &mut self,
        name: &str,
        value: impl Fn(i32) -> Result<T, String>,
    ) -> Result<Grid<P, T>, String> {
        let (width, height) = pair(self.field(name)?, ' ')?;
        let mut cells = Vec::with_capacity((width.max(0) * height.max(0)) as usize);
        let mut rows = 0;
        while rows < height {
            let (n, line) = self.next()?;
            let (k, runs) = match line.split_once("x ") {
                Some((k, runs)) => (number::<i32>(k)?, runs),
                None => (1, line),
            };
            let mut row: Vec<i32> = Vec::with_capacity(width.max(0) as usize);
            for run in runs.split_whitespace() {
                let (v, count): (i32, usize) = match run.split_once('*') {
                    Some((v, count)) => (number(v)?, number(count)?),
                    None => (number(run)?, 1),
                };
                row.extend(std::iter::repeat_n(v, count));
            }
            if row.len() != width as usize {
                return Err(format!("line {n}: {} cells, not {width}", row.len()));
            }
            for _ in 0..k {
                cells.extend_from_slice(&row);
            }
            rows += k;
        }
        if rows != height {
            return Err(format!("`{name}`: {rows} rows, not {height}"));
        }
        let cells = cells
            .into_iter()
            .map(value)
            .collect::<Result<Vec<T>, _>>()?;
        let mut cells = cells.into_iter();
        Ok(Grid::from_fn(width, height, |_| {
            cells.next().expect("as many cells as the grid")
        }))
    }
}

fn number<T: std::str::FromStr>(s: &str) -> Result<T, String> {
    s.parse().map_err(|_| format!("`{s}` is not a number"))
}

fn pair(s: &str, separator: char) -> Result<(i32, i32), String> {
    let (x, y) = s
        .split_once(separator)
        .ok_or_else(|| format!("`{s}` is not a pair"))?;
    Ok((number(x)?, number(y)?))
}

fn tile(s: &str) -> Result<TilePosition, String> {
    let (x, y) = pair(s, ',')?;
    Ok(TilePosition::new(x, y))
}
