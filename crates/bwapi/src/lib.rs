//! Rust bindings to BWAPI 4.4.0 for writing Starcraft: Brood War bots.
//!
//! A bot implements [`Bot<'game>`](Bot) and is exported with [`bot!`]. `'game` is
//! the match: handles (`Unit<'game>`, `Player<'game>`, …) are valid for the whole
//! match and can be stored in the bot, but cannot leave it, cross threads or meet
//! handles of another match.

mod bot;
mod generated;
mod position;
mod runtime;
mod session;

pub use bot::Bot;
pub use generated::*;
pub use position::{Position, TilePosition, WalkPosition};
pub use runtime::sets::{
    BulletQuery, BulletSet, BulletSetIter, ForceQuery, ForceSet, ForceSetIter, PlayerQuery,
    PlayerSet, PlayerSetIter, RegionQuery, RegionSet, RegionSetIter, UnitQuery, UnitSet,
    UnitSetIter,
};
pub use runtime::string::{BwStr, BwString};
pub use session::Context;

#[doc(hidden)]
pub mod __rt {
    pub use crate::session::{ErasedSession, StartFn, game_init, new_module, start};
}
