//! Game mechanics for [`bwapi`] bots: map analysis, building placement,
//! pathfinding and combat simulation. Decisions stay with the bot.
//!
//! # Credits
//!
//! Modules named after the C++ libraries are unofficial Rust ports of them. Their
//! authors neither endorse nor maintain the ports; report bugs of a port to this
//! crate.
//!
//! | Module | Original | Author | License |
//! |---|---|---|---|
//! | `bwem` | [BWEM-community](https://github.com/N00byEdge/BWEM-community) `9a63141` | Igor Dimitrijevic | MIT/X11, `LICENSE-BWEM` |
//! | `bweb` | [BWEB](https://github.com/Cmccrave/BWEB) v1.17 `d57980c` | Christian McCrave | MIT, `LICENSE-BWEB` |
//! | `fap` | [FAP](https://github.com/N00byEdge/FAP) `ac3c005` | Hannes Bredberg | MIT, `LICENSE-FAP` |
//! | `navigation` | A* and JPS of BWEB v1.17 | Christian McCrave; False.Genesis | MIT; public domain |
//!
//! `atan2` and `fmod`, which `core` lacks, are ports of [musl](https://musl.libc.org)
//! (MIT, `LICENSE-MUSL`); its `atan2` is that of FreeBSD msun, Sun Microsystems.

#![no_std]

extern crate alloc;

pub mod grid;

mod math;
mod position;

pub use position::PositionExt;
