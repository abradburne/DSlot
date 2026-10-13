#!/bin/bash
# Build for the RG DS+ and zip the files that go on the card.
set -euo pipefail
cd "$(dirname "$0")"

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
cargo zigbuild --release --target aarch64-unknown-linux-gnu.2.31

stage=dist/stage
rm -rf dist && mkdir -p "$stage"
cp -R card/Roms "$stage/"
cp target/aarch64-unknown-linux-gnu/release/dslot "$stage/Roms/APPS/dslot/dslot"
cp README.md "$stage/Roms/APPS/dslot/README.md"
find "$stage" -name .DS_Store -delete
(cd "$stage" && zip -qr "../dslot-$version-rgdsplus.zip" Roms)
echo "dist/dslot-$version-rgdsplus.zip"
