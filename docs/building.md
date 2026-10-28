# Building

DSlot is built for one platform at a time, chosen with a Cargo feature: `--features stock` or
`--features nibos`. Building with neither, or both, stops with a message saying so.

It builds on nibkit, the nibOS SDK's library ([github.com/abradburne/nibkit](https://github.com/abradburne/nibkit)),
and its `nib-cheevos` crate (RetroAchievements: rcheevos, which is C, so the build needs a C
compiler), which Cargo fetches at the tag `Cargo.toml` names. To work on both at once, point
Cargo at a checkout of your own for a while (`[patch."https://github.com/abradburne/nibkit"]`
in `.cargo/config.toml`, not committed, naming both `nibkit` and `nib-cheevos`, the latter at
`nib-cheevos/` in the checkout). The PKGBUILD does the same with `NIBKIT=path/to/nibkit
makepkg ...`.

## Checks

```sh
cargo fmt --check
cargo clippy --features nibos --all-targets -- -D warnings
cargo clippy --features stock --all-targets -- -D warnings
cargo test --features nibos
cargo test --features stock
```

The same run in CI (`.github/workflows/ci.yml`) on every push, plus the stock card's zip.

## The stock OS

Cross-built on a Mac (or Linux PC) with [cargo-zigbuild](https://github.com/rust-cross/cargo-zigbuild),
for the stock OS's glibc 2.31, and linked against the handheld's own SDL2:

```sh
brew install zig sdl2
cargo install cargo-zigbuild
rustup target add aarch64-unknown-linux-gnu
```

The library is not in the repository: copy it out of a stock image into `sysroot/libSDL2.so`.
It is `/usr/lib/libSDL2-2.0.so.0.3200.10` on the rootfs partition, which starts at sector
491520 of `RG-DS-PLUS-EN16GB-V1.0-20260915.IMG`:

```sh
debugfs -R "dump /usr/lib/libSDL2-2.0.so.0.3200.10 sysroot/libSDL2.so" "RG-DS-PLUS-EN16GB-V1.0-20260915.IMG?offset=$((491520*512))"
```

Then:

```sh
packaging/stock-card/package.sh
```

builds and zips `dist/DSlot-<version>-rgdsplus.zip`: `Roms/APPS/DSlot.sh`, the start at
power-on scripts, and `Roms/APPS/dslot/` with the program, `boot.sh` and `early.sh`, the
emulator descriptions, the README, and the licences that go with every copy: LICENSE,
OFL.txt (the font's) and THIRD-PARTY.md (the crates', [licences.md](licences.md)). The nibOS
package puts the same three in `/usr/share/licenses/dslot/`.

## nibOS

Natively, on an aarch64 Arch Linux ARM machine (nibOS's build machine), as an Arch package:

```sh
cd packaging/nibos
PKGDEST="$PWD/out" BUILDDIR="$HOME/makepkg-build" makepkg -f --nodeps
sudo pacman -U out/dslot-*.pkg.tar.*     # on the device
```

The package is built from the checkout (no download); the tests run in `check()`. It needs
`sdl2-compat` and Rust to build. What it installs, and what nibOS adds: docs/nibos.md.

For a quick try without packaging, in the build machine:

```sh
cargo build --release --features nibos
```

and copy `target/release/dslot` over `/usr/bin/dslot` on the device, then
`sudo systemctl restart dslot` (not while a game is running: DSlot stops with it).

## Desktop preview

Either feature builds and runs on a Mac with Homebrew's SDL2, or on Linux under X or Wayland,
the two screens stacked:

```sh
cargo build --release --features nibos
target/release/dslot --make-demo /tmp/demo       # a Roms folder of tiny fake games
target/release/dslot --roms /tmp/demo
```

Arrow keys are the D-pad, Return or Space A, Backspace B, Tab X, F Y, L and R the shoulders,
S Select, D Start, \` (backquote) the Back key, Escape leaves; the mouse is the touch screen. Headless, to a PNG (how the screenshots in
`docs/` are made; `--press` keys first, one a frame):

```sh
SDL_VIDEODRIVER=dummy target/release/dslot --roms /tmp/demo --press Tab --shot menu.png
target/release/dslot --skeleton-shot skeleton.png
```

The nibOS build's files go to the XDG folders of whoever runs it (set `HOME` to keep a preview
apart); the stock build's go beside the binary.

The headless preview is one window with both screens stacked, and a shot on nibOS stacks its two
panels the same way, the top one first (through the unit, as DSlot's user can't take the panels
from a shell: `DSLOT_ARGS=--shot /tmp/x.png --press Tab` in `/etc/default/dslot`, then
`systemctl restart dslot`). `DSLOT_TWO_WINDOWS=1` makes it a
window per screen, as on nibOS, where a texture drawn on the wrong screen's renderer fails
(docs/architecture.md). `tests/picker.rs` runs the system picker from every page that way. A
key name SDL doesn't know (`wait`) is a frame with nothing pressed, for artwork or a resume
picture to come in before the shot. Headless runs never rest (README, "Resting").

## The sounds

`assets/insert.pcm` and `assets/eject.pcm` were cut from a recording of a real DS card slot with
`tools/cut_sounds.py` (it needs ffmpeg); `docs/insert.wav` and `docs/eject.wav` are the same, to
listen to.

## Releasing

1. Update `version` in `Cargo.toml` and `CHANGELOG.md`.
2. Run the checks, build the stock zip and the nibOS package, try both on the device.
3. Tag `vX.Y.Z`, and attach the zip to the GitHub release.
