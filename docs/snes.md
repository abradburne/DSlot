# SNES

DSlot's **SNES** page uses `Roms/SFC` on either card, matching the stock OS and nibcase's
system id. Games may be `.sfc`, `.smc` or `.zip` (extensions are case insensitive).
`Moon Bay.smc.zip` uses `Moon Bay` for its artwork, settings key and saves directory.

The page appears on the device only when an installed emulator describes `SFC`. Desktop
previews show it without an emulator, as with the other consoles. `--make-demo` includes two
fabricated SNES headers to try the shelf; these are not playable games.

## Artwork and metadata

The carousel draws a SNES cart in one of its two shapes, each measured from a photo of a
real one: the US cart (purple-grey and squared off, grooved wings either side of the label,
black label panels with a red strip at the top right) or the Super Famicom's, as sold in Japan,
Europe and everywhere else (light grey, a curved top, short slots on the upper sides, a white
label with the four button colours as dots). Both have the three-part grip and the two screws.
Which one is the SNES page's **SNES carts** setting: **Match the game's region** (the default:
the header's destination code, else the region in the file's name, "(USA)" or "(Japan)"; with
neither, the system's locale), **US** or **Super Famicom**. No logos or trademarks are drawn.
Boxes use the landscape SNES shape; the grid fits them within each cell without
stretching. The insertion animation uses the existing wide cartridge slot.

As on the other pages, `Carts/<name>.png` replaces the entire cart, `Labels/<name>.png` replaces
its label and `Imgs/<name>.png` supplies its cover (and label when no separate label exists).
Without artwork DSlot prints a label and cover from the game's name and publisher.

The file's tidied name is the displayed title. Publishers and optional expanded-header game
codes come from LoROM, HiROM or ExHiROM headers, with or without a 512-byte copier prefix.
Candidate headers are checked against their mapping and title; checksum pairs and reset
vectors help choose between plausible headers. A missing or damaged header leaves the game
listed by filename. ZIPs are read without extracting files, skipping directory and non-ROM
members and streaming only as far as the candidate headers. Metadata is cached in `snes.bin`.

Format sources: [PVSnesLib's SNES ROM Header documentation](https://github.com/alekmaul/pvsneslib/wiki/SNES-ROM-Header),
[SNESdev's ROM file formats](https://snes.nesdev.org/wiki/ROM_file_formats).
No emulator source was used for the header reader.

## Launching

The stock build ships `snes9x.toml`: the stock RetroArch launcher and
`/mnt/vendor/deep/retro/cores/snes9x_libretro.so` must both be installed.

On nibOS, DSlot does not ship or select a core. Each emulator/core package installs its own
TOML description under `/usr/share/dslot/emulators/`. Install `nibcase` and
`nibcase-snes9x`; its package hook queries the installed core's actual
options and writes its descriptor. The minimal
[nibcase-snes9x.toml](examples/nibcase-snes9x.toml) illustrates the working CLI:

```text
/usr/bin/nibcase --core /usr/lib/libretro/snes9x_libretro.so --system SFC \
  --saves "/mnt/roms/Saves/SFC/Moon Bay (USA)" "/mnt/roms/Roms/SFC/Moon Bay (USA).sfc"
```

DSlot notices descriptor and availability changes within about two seconds while on the shelf,
including additions/removals and changed core options. It rescans the relevant console folders
and rebuilds the menus, preserving the selected game where possible. Changes made during a
game are picked up when that game returns. No DSlot configuration edit is needed.

nibcase provides software video, stereo audio, the shared nibOS menu, battery saves and six
manual states plus automatic resume. Its frontend is separate from the core packages;
Snes9x's own noncommercial licence applies to its separately installed library. DSlot does
not bundle an emulator core. See the sibling nibcase project's README and validation record.

Settings and saves already follow the normal rules: `[console.SFC]`,
`[game."SFC/<ROM name>"]`, and `Saves/SFC/<ROM name>/`. Core choice, declared options,
favourites, ratings, play time and save deletion use the same paths as the other consoles.

## Verification

```sh
cargo fmt --check
cargo clippy --locked --features nibos --all-targets -- -D warnings
cargo clippy --locked --features stock --all-targets -- -D warnings
cargo test --locked --features nibos
cargo test --locked --features stock
```

Tests use synthetic ROM headers and ZIP files: mapping modes, copier prefixes, old and new
licensees, malformed headers, scanning and metadata caching, `.smc.zip` naming, the nibcase
command and saves directory, cartridge windows, and landscape thumbnail sizing.

Shelf and grid PNG previews were also rendered and inspected.

On the device, check the shelf and grid with your own art, touch and insertion, then launch
and return using an installed `SFC` emulator. nibcase has also been validated independently on both panels with a freely released test ROM;
physical controls, speaker/headphone listening and suspend remain part of the hardware checklist.
