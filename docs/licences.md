# Licences

DSlot is free software under the **GNU General Public License, version 3 or (at your option)
any later version** (GPL-3.0-or-later): [LICENSE](../LICENSE) is the FSF's text, unchanged.
Anyone who passes DSlot on, as source or as a built program, passes it on under those terms,
with its source.

What is not DSlot's own keeps its own licence:

| Part | Licence | Where |
|---|---|---|
| nibkit (nibOS's SDK: settings, the shared menu, the radios) | MIT | [github.com/abradburne/nibkit](https://github.com/abradburne/nibkit), by tag |
| Roboto Condensed, embedded for the text | SIL Open Font License 1.1 | [assets/OFL.txt](../assets/OFL.txt), shipped beside every copy |
| The system cards' console photos | public domain, or CC BY 4.0 as credited | [photo-credits.md](photo-credits.md) |
| The crates built into the program | their own; listed below | [THIRD-PARTY.md](../THIRD-PARTY.md) |
| SDL2 | zlib | the system's `libSDL2`, linked at run time, not built in |
| The emulators DSlot starts | their own | separate programs and packages |

The emulators are separate programs that DSlot starts with a command line, so their licences
and DSlot's do not mix. The OFL covers the font file alone: embedding a font in a program does
not make the program a derivative of the font.

## The crates

All crates in the resolved dependency graph (`cargo metadata` for
`aarch64-unknown-linux-gnu`, with `--features nibos` and with `--features stock`; 109 crates
in all) were checked against GPL-3.0. Each is under a permissive licence that GPL-3.0 code may
use, and there is no GPL-2.0-only crate, no copyleft crate other than DSlot itself, and none
under a licence GPL-3.0 cannot take:

| Licence | Crates |
|---|---|
| MIT OR Apache-2.0 (in either spelling) | most of the graph: serde, toml, png, zip and their kin |
| MIT | sdl2, lz4_flex, the zbus stack (nibOS only) and others; nibkit (MIT, with the OFL font) |
| Apache-2.0 alone | zopfli. Apache-2.0 is compatible with GPL-3.0 (not with GPL-2.0) |
| Zlib | foldhash, zlib-rs |
| MIT AND Zlib | sdl2-sys |
| MIT OR Apache-2.0 OR Zlib | fontdue, zune-core, zune-jpeg, miniz_oxide |
| Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | rustix, linux-raw-sys |
| 0BSD OR MIT OR Apache-2.0 | adler2 |
| Unlicense OR MIT | memchr |
| (MIT OR Apache-2.0) AND Unicode-3.0 | unicode-ident |

Where a crate offers a choice, DSlot takes it under MIT, or Apache-2.0 for a crate that
offers no MIT.

## Notices that go with every copy

MIT, Apache-2.0, Zlib and the OFL each ask for their licence and copyright notice to go with
copies. [THIRD-PARTY.md](../THIRD-PARTY.md) lists every crate with its licence and carries the
texts the crates ship. Both packages carry it, with LICENSE and OFL.txt:

- nibOS (`packaging/nibos/PKGBUILD`): `/usr/share/licenses/dslot/`
- the stock card (`packaging/stock-card/package.sh`): `Roms/APPS/dslot/`

When the dependencies change (`Cargo.lock`), regenerate it and check for a licence not in the
table above:

```bash
python3 tools/third_party.py > THIRD-PARTY.md
```
