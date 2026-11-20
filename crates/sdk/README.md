# bwapi-sdk

Rust ports of the C++ libraries that StarCraft: Brood War bots rely on, for
bots written with [`bwapi`](https://crates.io/crates/bwapi):

| Module | What it does | Ported from | Author | License |
|---|---|---|---|---|
| `bwem` | Map analysis: areas, choke points, bases | [BWEM-community](https://github.com/N00byEdge/BWEM-community) `9a63141` | Igor Dimitrijevic | MIT/X11 |
| `bweb` | Building placement, walls, pathfinding | [BWEB](https://github.com/Cmccrave/BWEB) v1.17 `d57980c` | Christian McCrave | MIT |
| `fap` | Combat simulation | [FAP](https://github.com/N00byEdge/FAP) `ac3c005` | Hannes Bredberg | MIT |

The JPS pathfinder inside BWEB was written by False.Genesis and is public
domain. `atan2` and `fmod` are ported from [musl](https://musl.libc.org)
(MIT), because `core` doesn't have them.

These ports are unofficial. The original authors have nothing to do with them,
so please report bugs here, not upstream.

## Ported as is

For now each library is ported as is, bugs and all: the port gives the same
results as the C++ original. We checked that by running both on the same
games, and the tests keep checking it on 266 maps.

That's temporary. Next releases will start fixing the bugs of the originals,
and from then on the ports will give different results: faster, and more
correct.

The API is not a copy of the C++ one. It uses ids instead of pointers, and
full names instead of abbreviations. Where a port does something different
from the original on purpose, its docs say so.

The crate is `no_std` (it needs `alloc`). Only the `adapter` module talks to
the game; everything else works on plain data, so you can run it without
StarCraft.

## Running the tests

```sh
cargo test -p bwapi-sdk
```

This runs on 18 real maps: the 17 from BWEM-community's own tests plus Road
War. To run on all 266 maps:

```sh
TEST_MAPS=all cargo test -p bwapi-sdk
```

The tests don't need the game. `tests/data` holds what BWAPI reports for each
map at game start, and the expected results of BWEM, BWEB and FAP. If you
change something that is supposed to change the results, regenerate them:

```sh
UPDATE_ANSWERS=1 TEST_MAPS=all cargo test -p bwapi-sdk
```

Besides comparing with expected results, the tests check general properties
that any correct result should have. Known bugs of the originals that break
these properties are listed in `tests/data/known.txt`. A test fails both on a
new problem and on a listed one that is gone, so fixes don't go unnoticed.

## License

MIT, same as the originals. See `LICENSE-BWEM`, `LICENSE-BWEB`, `LICENSE-FAP`
and `LICENSE-MUSL`.
