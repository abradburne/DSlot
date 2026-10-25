#!/bin/bash
# Build DSlot for the stock OS and zip what goes on the games card: Roms/APPS/DSlot.sh and
# the folder dslot/ beside it (the program, its scripts, the emulator descriptions and the
# README). Cross-built on a Mac or Linux PC with cargo-zigbuild,
# against the handheld's own libSDL2 in sysroot/ (docs/building.md).
#
#   packaging/stock-card/package.sh        -> dist/DSlot-<version>-rgdsplus.zip
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
cd "$here/../.."

[ -f sysroot/libSDL2.so ] || { echo "no sysroot/libSDL2.so: see docs/building.md" >&2; exit 1; }
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
# glibc 2.31: the stock OS's.
cargo zigbuild --release --features stock --target aarch64-unknown-linux-gnu.2.31

stage=dist/stage
rm -rf "$stage" "dist/DSlot-$version-rgdsplus.zip" && mkdir -p "$stage"
cp -R "$here/Roms" "$stage/"
app="$stage/Roms/APPS/dslot"
cp target/aarch64-unknown-linux-gnu/release/dslot "$app/dslot"
cp README.md "$app/README.md"
# DSlot's licence, and the crates' notices, which go with every copy too.
cp LICENSE THIRD-PARTY.md "$app/"
# The embedded Roboto Condensed's licence, which goes with every copy.
cp assets/OFL.txt "$app/OFL.txt"
find "$stage" -name .DS_Store -delete
(cd "$stage" && zip -qr "../DSlot-$version-rgdsplus.zip" Roms)
echo "dist/DSlot-$version-rgdsplus.zip"
