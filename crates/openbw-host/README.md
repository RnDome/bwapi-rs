# openbw-host

Plays real games of the [OpenBW fork of BWAPI](https://github.com/openbw/bwapi)
inside a test process, without `BWAPILauncher`, and uses them to check the
ports of `bwapi-sdk` against the C++ libraries they were ported from.

This crate lives on a branch of its own on top of `sdk`. It needs a build of
OpenBW and StarCraft's MPQ files, which cannot be distributed, so it is not in
`default-members` and CI excludes it from `--workspace`.

## What you need

| What | Where | Needed for |
|---|---|---|
| A built checkout of `github.com/openbw/bwapi` (`48124ba8`) with `libBWAPI.so`, `libBWAPILIB.so` and `libOpenBWData.so` in `build/lib`, patched as below | `OPENBW_BWAPI_DIR` | building anything |
| `StarDat.mpq`, `BrooDat.mpq`, `Patch_rt.mpq` | `OPENBW_MPQ_PATH` | running anything |
| A checkout of [BWEM-community](https://github.com/N00byEdge/BWEM-community) (`9a63141`) | `BWEM_DIR` | `bwem_parity` |
| A checkout of [BWEB](https://github.com/Cmccrave/BWEB) v1.17 (`d57980c`), together with `BWEM_DIR` | `BWEB_DIR` | `bweb_parity` |
| A checkout of [FAP](https://github.com/N00byEdge/FAP) (`ac3c005`) | `FAP_DIR` | `fap_parity` |

The submodule `crates/bwapi/vendor/bwapi` must be checked out: the C++
libraries are built against its headers, as bots are.

The three reference checkouts are optional. Without one, its comparison is
not built and its test file has no tests; nothing fails.

### Local patches of the reference checkouts

The C++ libraries do not build and run as they are. The checkouts need the
patches in `patches/`, each change marked `Local patch` in the source. Apply
each to a clean checkout of the commit above, from the root of this
repository:

```sh
git -C ~/scbw/BWEM-community apply "$PWD/crates/openbw-host/patches/bwem-community.patch"
git -C ~/scbw/BWEB           apply "$PWD/crates/openbw-host/patches/bweb.patch"
git -C ~/scbw/FAP            apply "$PWD/crates/openbw-host/patches/fap.patch"
```

- **`bweb.patch`** (`Source/`): builds with GCC (v1.17 is written for MSVC);
  uses the choke point width of the fork of BWEM in McRave; does not read or
  write past its grids on maps 256 tiles wide or high; makes no start blocks
  where the main has no choke point; a faster search in `PathFind.cpp`.
- **`bwem-community.patch`** (`BWEM/`): `ChokePoint::Width`, which BWEB calls;
  the type of a neutral, for the checks; two fixes of dangling and
  unterminated lists of neutrals.
- **`fap.patch`** (`FAP/include/FAP.hpp`): `simulate` returns how many frames
  it ran and why it stopped. What it simulates is unchanged. The file has
  CRLF line ends and so has the patch.

### Patches of OpenBW: games that repeat

Two patches make a game played twice the same game. One is for the checkout
in `OPENBW_BWAPI_DIR` (`48124ba8`), the other for the checkout of
[OpenBW](https://github.com/OpenBW/openbw) (`4b046d5`) that the fork is built
against, `OPENBW_DIR` of its CMake. Rebuild the fork after applying them:

```sh
git -C ~/scbw/bwapi  apply "$PWD/crates/openbw-host/patches/openbw-bwapi.patch"
git -C ~/scbw/openbw apply "$PWD/crates/openbw-host/patches/openbw.patch"
```

- **`openbw-bwapi.patch`** (`BWAPI/Source/BWAPI/UnitImpl.{h,cpp}`): `UnitImpl`
  objects come from a fixed pool instead of the heap. A unit created during
  the game then has the same address in every run, and `Unitset`s, which hash
  by address, come out in the same order. The files have CRLF line ends and
  so has the patch.
- **`openbw.patch`** (`sync.h`): two environment variables of a network game.
  `OPENBW_GAME_SEED` is the seed of the game instead of the one made from the
  clients. With `OPENBW_FIXED_SLOTS` set, every client stays in the slot it
  took, and so at the start location of that slot.

This crate relies on `openbw.patch`: `cpp/host.cpp` sets `OPENBW_FIXED_SLOTS`
for every game, so that the bot starts at the first start location of the
map, and `OPENBW_GAME_SEED` to 1 unless it is given, so that the race of the
computer, and with it the ids of the neutrals, is the same on every run. On
an unpatched OpenBW both are ignored and the fixtures do not match the game.
The comparisons were run on a fork built with both patches.

## Running

From the root of the repository, in release: unoptimised, the comparisons
take several times longer.

```sh
export OPENBW_BWAPI_DIR=~/scbw/bwapi
export OPENBW_MPQ_PATH=~/scbw/bwapi/build/bin
export BWEM_DIR=~/scbw/BWEM-community
export BWEB_DIR=~/scbw/BWEB
export FAP_DIR=~/scbw/FAP

# The default maps: about a minute.
cargo test -p openbw-host --release

# Every map of the repository, `maps/`: about four minutes.
PARITY_MAPS=all cargo test -p openbw-host --release

# Every map under a directory; an absolute path, tests run in the crate's directory.
PARITY_MAPS=$PWD/maps/scai cargo test -p openbw-host --release

# One comparison.
cargo test -p openbw-host --release --test bweb_parity
```

| Variable | When | Meaning |
|---|---|---|
| `OPENBW_BWAPI_DIR` | build | the checkout of the OpenBW fork of BWAPI |
| `OPENBW_BWAPI_LIB_DIR` | build, optional | its libraries, if not in `build/lib` |
| `BWEM_DIR`, `BWEB_DIR`, `FAP_DIR` | build, optional | the reference checkouts |
| `OPENBW_MPQ_PATH` | run | the directory with the MPQ files |
| `PARITY_MAPS` | run, optional | `all` or a directory: the maps the comparisons run on |
| `OPENBW_GAME_SEED` | run, optional | the seed of the games, 1 by default; needs `openbw.patch` |
| `UPDATE_FIXTURES` | run, optional | writes the fixtures instead of checking them, see below |

The build variables are read by `build.rs`: changing one rebuilds the crate.
Without `OPENBW_BWAPI_DIR` the build stops with a message that says so.

## The tests

| Test | What it checks |
|---|---|
| `bwem_parity` | the port of BWEM against BWEM-community on the same game: the analysis, and the analysis after each neutral is destroyed; C++ is also checked for the properties of the tests of `bwapi-sdk` |
| `bwem_maps` | BWEM on real maps against the numbers of the tests of BWEM-community |
| `bweb_parity` | the port of BWEB against BWEB v1.17 as each race: walkability, used and reserved tiles at each stage, placement, stations, walls, blocks, paths and ground distances |
| `fap_parity` | the port of FAP against FAP on random fights, frame by frame, with and without tank splash |
| `fixtures` | the fixtures of `bwapi-sdk`, `crates/sdk/tests/data/**/input.txt`, against what BWAPI reports at the start of a game, and against `adapter::map_input` and `adapter::start_state` |
| `game` | a Rust bot plays Showdown in this process |

`bwem_parity`, `bweb_parity` and `fixtures` start a process per map, and per
race where the race matters; the tests named `one_map` are those processes
and are ignored when run directly.

`bweb_parity` does not compare a resource depot where units or creep are
near, and allows defense angles to differ in the last bits: see the top of
`tests/bweb_parity.rs`.

## Writing the fixtures anew

The tests of `bwapi-sdk` read fixtures, not maps. To write them from the
game, after a change of what the adapter reads or for new maps:

```sh
UPDATE_FIXTURES=1 PARITY_MAPS=all cargo test -p openbw-host --release --test fixtures
```

This overwrites `input.txt` under `crates/sdk/tests/data`. The answers next to
them are written by the tests of `bwapi-sdk` itself (`UPDATE_ANSWERS`), after
the port is known to match C++ here.
