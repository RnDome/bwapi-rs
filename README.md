# About

Idiomatic Rust bindings to the [BWAPI library](http://bwapi.github.io/) for writing bots for StarCraft: Brood War in the [Rust language](https://www.rust-lang.org/). This project has nothing to do with StarCraft 2.

The full API of BWAPI 4.4.0 is supported. Bots built with it run in [OpenBW](https://github.com/openbw/openbw) on Linux and in the original StarCraft 1.16.1 with BWAPI 4.4.0.

# Running a bot

A bot runs in two hosts:
- on Linux, in [OpenBW](https://github.com/openbw/openbw), an open-source Brood War engine with a BWAPI backend (steps 1–5);
- in the original StarCraft 1.16.1 with BWAPI 4.4.0, on Windows or under wine (step 1, then [Running in StarCraft](#running-in-starcraft)).

Both use the same bot source. OpenBW needs the data files of StarCraft: Brood War 1.16.1, so the game is installed first (under wine on Ubuntu).

## 1. Install StarCraft: Brood War under wine

You need:
- an installer of StarCraft: Brood War 1.16.1;
- [`BWAPI_Setup.exe`](https://github.com/bwapi/bwapi/releases/download/v4.4.0/BWAPI_Setup.exe) of BWAPI 4.4.0.

```sh
sudo apt install wine
wine sc_bw_installer.exe
wine BWAPI_Setup.exe
```

The game lands in `~/.wine/drive_c/Program Files (x86)/Starcraft`. To play it with BWAPI (no Rust bot, the regular game):

```sh
wine ~/.wine/drive_c/Program\ Files\ \(x86\)/Starcraft/BWAPI/Chaoslauncher/Chaoslauncher.exe
```

## 2. Build OpenBW

```sh
sudo apt install build-essential cmake libsdl2-dev
mkdir -p ~/scbw && cd ~/scbw
git clone https://github.com/openbw/openbw
git clone https://github.com/openbw/bwapi
cd bwapi
mkdir build
cd build
cmake .. -DCMAKE_BUILD_TYPE=Release -DOPENBW_DIR=../../openbw -DOPENBW_ENABLE_UI=1
make
```

`-DOPENBW_ENABLE_UI=1` opens a window with the game; without it OpenBW runs headless.

## 3. Copy the game data to OpenBW

OpenBW reads the data files from its working directory. File names are case-sensitive.

```sh
cd ~/scbw/bwapi/build/bin
STARCRAFT="$HOME/.wine/drive_c/Program Files (x86)/Starcraft"
cp "$STARCRAFT/StarDat.mpq" .
cp "$STARCRAFT/BrooDat.mpq" .
cp "$STARCRAFT/patch_rt.mpq" ./Patch_rt.mpq
cp -r "$STARCRAFT/Maps" .
mkdir -p bwapi-data
```

## 4. Build the bot

```sh
git clone --recursive https://github.com/RnDome/bwapi-rs
cd bwapi-rs
cargo build -p bot_rust
```

The bot is `target/debug/libbot_rust.so`; its source is `examples/bot_rust`. A bot in C over the same library is `examples/bot_c`.

## 5. Run

Write `~/scbw/bwapi/build/bin/bwapi-data/bwapi.ini`:

```ini
[ai]
ai = /path/to/bwapi-rs/target/debug/libbot_rust.so

[auto_menu]
auto_menu = SINGLE_PLAYER
map = Maps/(2)Bottleneck.scm
race = Zerg
enemy_race = Terran
```

and start OpenBW:

```sh
cd ~/scbw/bwapi/build/bin
./BWAPILauncher
```

Without `auto_menu` and `map` OpenBW stops with `file_reader: failed to open  for reading`.

Every setting can also be given as an environment variable `BWAPI_CONFIG_<SECTION>__<KEY>`, for a single run:

```sh
BWAPI_CONFIG_AUTO_MENU__MAP='Maps/(4)Dark Crystal.scm' ./BWAPILauncher
```

All settings: [BWAPI configuration](https://github.com/bwapi/bwapi/wiki/Configuration).

OpenBW has no built-in computer player: in a single-player game the opponent does nothing. For a real opponent run two `BWAPILauncher`s with `auto_menu = LAN` (see the [OpenBW BWAPI README](https://github.com/openbw/bwapi#multiplayer)).

## Running in StarCraft

BWAPI 4.4.0 loads a bot as a 32-bit MSVC DLL. On Linux it is cross-compiled with [cargo-xwin](https://github.com/rust-cross/cargo-xwin), which downloads the MSVC CRT and the Windows SDK and builds with `clang-cl` and `lld-link`.

```sh
sudo apt install clang lld llvm
cargo install --locked cargo-xwin
cd bwapi-rs
rustup target add i686-pc-windows-msvc
```

`rustup target add` inside the repository adds the target to the toolchain of `rust-toolchain.toml`.

Build the bot:

```sh
XWIN_CACHE_DIR=$HOME/.cache/cargo-xwin-vs2019 \
  cargo xwin build -p bot_rust --target i686-pc-windows-msvc --xwin-version 16 --xwin-arch x86
```

The bot is `target/i686-pc-windows-msvc/debug/bot_rust.dll`.

- `--xwin-arch x86`: by default cargo-xwin downloads only the 64-bit libraries.
- `--xwin-version 16`: the STL of Visual Studio 2022 (the default) requires clang 19 or newer; the one of Visual Studio 2019 builds with older clang, e.g. clang 14 of Ubuntu 22.04. With clang 19+ drop this flag.
- `XWIN_CACHE_DIR`: cargo-xwin downloads once per cache directory and does not re-download for other flags, so a separate directory keeps this SDK apart from the default one. To change the flags of an existing cache, delete the directory.

Write `~/.wine/drive_c/Program Files (x86)/Starcraft/bwapi-data/bwapi.ini`:

```ini
[ai]
ai = Z:\path\to\bwapi-rs\target\i686-pc-windows-msvc\debug\bot_rust.dll

[auto_menu]
auto_menu = SINGLE_PLAYER
map = maps\(2)Bottleneck.scm
race = Zerg
enemy_race = Terran
```

`Z:` is the root of the Linux file system under wine. The `RELEASE` injector reads `ai`, the `DEBUG` one reads `ai_dbg`.

Start Chaoslauncher, check `BWAPI 4.4.0 Injector [RELEASE]` and press `Start`:

```sh
wine ~/.wine/drive_c/Program\ Files\ \(x86\)/Starcraft/BWAPI/Chaoslauncher/Chaoslauncher.exe
```

If BWAPI cannot load the bot, the prefix may lack `MSVCP140.dll`: install the Visual C++ 2017 x86 runtime (`winetricks vcrun2017`).

# Contribution

Ideas and/or contributions are very welcome. Please feel free contacting us by email or using Issues.
