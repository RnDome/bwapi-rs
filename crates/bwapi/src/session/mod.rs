//! When a handle is valid: the match brand `'game` (brand.rs), the callback
//! `Context` with its `'frame` (context.rs), and the lifecycle of the session that
//! owns the bot for one match (lifecycle.rs).

mod brand;
mod context;
mod lifecycle;

pub(crate) use brand::Brand;
pub use context::Context;
pub(crate) use context::FrameToken;
pub use lifecycle::{ErasedSession, StartFn, game_init, new_module, start};
