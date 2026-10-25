# System cards

Every page has a card on the top screen, shown in the system picker (Select) and on the menu's
page for its console: a Top Trumps style card for the system, or, for the Apps page, one about
the device itself.

## A system's card

`data/systems/<key>.toml`, one per system, built into DSlot (`src/systems.rs`, `FILES`). `<key>`
is the page's key (`Console::key` in `src/rom.rs`: `ds`, `gba`, `snes`, `playstation`, ...).

```toml
# Atari Lynx: the system card's facts, from https://en.wikipedia.org/wiki/Atari_Lynx
# (read 21 October 2026). The bio and fun fact are DSlot's own words.
name = "Atari Lynx"
maker = "Atari (designed by Epyx)"
source = "https://en.wikipedia.org/wiki/Atari_Lynx"
bio = "Designed by Epyx and launched by Atari in North America on 1 September 1989, ..."
fact = "It was designed by RJ Mical and Dave Needle, who had worked on the Amiga."

[[stats]]
label = "Released"
value = "1989"
bar = 1989
```

- **Every figure is from `source`**, printed small at the card's foot, and the header comment
  says when it was read. A stat the source doesn't give is left out, never estimated. The bio
  (up to about 45 words) and the fun fact (one sentence) are in DSlot's own words, and only say
  what the source supports.
- **Stats**: up to seven, from the usual labels (Released, Launch price, Sold, Processor,
  Memory, Screen, Games) so the bars compare like with like. `value` is what the card prints
  (about 32 characters at most); `bar` is the number its bar is drawn from, against every
  card's same label: linear for Released and Launch price, logarithmic for the rest (memory in
  KB, sales in millions, a screen as width × height). Leave `bar` out where values don't
  compare: a price in yen, a count for one region only.
- **Photo**: `data/systems/photos/<key>.png`, 340 × 224, the console fitted on white. Only
  freely licensed photographs: public domain, or Creative Commons with the credit it asks for
  (cropping is noted). Each photo's credit is in `PHOTOS` in `src/systems.rs` and is printed on
  the card, and `data/systems/photos/CREDITS.md` lists its Commons page, author and licence.
  The package installs that file as `/usr/share/doc/dslot/photo-credits.md`. No logos,
  trademarks or box art as the subject.
- **Not consoles**: the Ports card (`ports.toml`) has no stats and no photo.

### Adding a system

1. Its page: `Console` in `src/rom.rs` (folder, label, key, extensions), and wherever the
   compiler then asks (artwork shapes, colour, status bar).
2. Its card: `data/systems/<key>.toml`, added to `FILES` in `src/systems.rs`.
3. Its photo: the PNG, a `PHOTOS` row with its credit, and a line in `CREDITS.md`.

The tests in `src/systems.rs` enforce this: `every_page_has_a_card` fails for a page with no
card, `every_card_cites_its_figures` for a card without an `https://` source, bio, fact and five
stats, and `every_card_has_its_photo` for a missing photo or credit (Ports aside).

## The device's card (Apps)

The Apps page's card is about the handheld you are holding (`src/device.rs`).

- **What it is:** its board's facts, from `data/devices/<board>.toml` (`board`, `name`,
  `source`, and `[[specs]]` of `label` and `value`: chip, CPU, GPU, memory, screens, storage).
  The board is the one nibOS names in `/usr/lib/nibos/board.conf` (`NIBOS_BOARD`). The stock
  OS build is always the RG DS+. Another board gets its own file, added to `BOARDS`.
- **Its system:** nibOS's release (`/etc/nibos-release`: `VERSION`, `BUILD_DATE`) where there
  is one. Otherwise the version and build date of the `nibos-hw` package, from pacman's
  records, or else the system's own name (`/etc/os-release`).
- **How it is now:** read every few seconds while the card is up. The battery's charge and what
  it is doing, and battery protect (`nibkit::battery`). The room free on each card the games are
  on (one row per file system). How long it has been on, sleep included. The time played in
  all. What is not known is left out: the stock OS keeps the battery to itself, and the RK817
  reports neither a cycle count nor a worn capacity.
- **What has been played:** how many games, on how many systems, and the game played most. The
  number of apps, and the one used last.
- **The picture** is the device drawn by DSlot (`draw_device`), not a photo.
