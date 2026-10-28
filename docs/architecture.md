# Architecture

DSlot is one Rust program on SDL2. What any app on the handheld needs (where the screens
are, the buttons' labels, touch, brightness and volume, app folders, layered settings, the
emulator description files) is **nibkit**, the nibOS SDK's library ([github.com/abradburne/nibkit](https://github.com/abradburne/nibkit),
MIT), which DSlot builds on; what is DSlot's own is here. It draws everything itself:
carts, boxes and text are composed in software (`gfx`, `art`) on worker threads, uploaded as
textures, and put together on the screens each frame by SDL's GLES renderer.

```
main ─── platform (stock | nibos) ── where things are, start-up, power
  │
  ├── rom ── scan Roms/<SYSTEM>, read headers ── cache/<system>.bin
  ├── art ── cart shells, carts, boxes ── gfx (images, PNG, fonts)
  ├── emulator ── descriptions, their options and actions
  ├── settings ── system / console / game, settings.toml ── launch (the command, running it)
  ├── stats ── stats.tsv
  ├── kms ── the start-up skeleton, straight on the CRTCs
  └── ui ── screens, shelf (frame loop) ── input, menu, draw, layout, sound
                                         └── loader (artwork threads) ── art
```

## A run

1. **Start** (`main.rs`). Read the arguments; on nibOS put the skeleton on screen (`kms`,
   `ui::skeleton`) unless the power-on splash is up. Then, all at once: Mesa loaded on a thread
   (nibOS), each page's games listed on a thread (`rom::scan`), the cart shell drawn on
   another, and SDL, the windows and renderers made on the main thread (`ui::open`). Load the
   emulator descriptions, settings and play times.
2. **The shelf** (`ui::run`), until a cart goes into the slot, DSlot is left, or the device is
   powered off.
3. **A game** (`main::play`). Close SDL, which lets go of both panels; pick the emulator, put
   its command together and run it (`launch`); add the time played to `stats.tsv`; open SDL
   again and show the shelf, the game's cart popping out of the slot.

## Modules

| | |
|---|---|
| `main.rs` | Arguments, start-up, the loop of shelf and games, the start-up marks. |
| `platform/` | What DSlot does differently on the stock OS and nibOS, chosen by Cargo feature (which also picks nibkit's): where it keeps its files and saves, fonts, the Menu button, start-up, start at power-on (`stock/power_on.rs`: the stock boot script's hook). The device's controls (volume, brightness, Wi-Fi) through nibkit. |
| `rom.rs` | The pages (`Console`: every system, Ports and Apps, each with its folder, extensions, header reader, serial prefix, key), scanning their folders (an .m3u's discs aren't games of their own), and the known-games file that saves reading every ROM at every start. The demo ROMs. `rom/snes.rs` reads SNES headers; `rom/ports.rs` names and pictures PortMaster's ports from their folders. |
| `systems.rs` | The system cards: `data/systems/*.toml` and their photos, built in; the bars' scales. docs/system-cards.md. |
| `device.rs` | The Apps page's card: the board's facts (`data/devices/`), the system's version, the battery, the cards' room, uptime, what has been played. |
| `resume.rs` | A game's resume point as its emulator left it: DSperate's, inkdot's and nibcase's formats, the picture and when it was saved. docs/emulators.md, "Resume points". |
| `art.rs` | Cart shells (drawn, not images), carts with their labels, boxes in their cases, grid covers (cached). Which systems have a shell, and so a page. |
| `gfx.rs` | `Img` (RGBA in memory), drawing and blending, PNG in and out, the fonts (embedded Roboto Condensed, the system's CJK face as a fallback). |
| `loader.rs` | Two worker threads composing artwork, asked for it in order of need; out-of-date answers are dropped. |
| `library.rs` | A page's games in the order shown (sort, favourites filter) and letter jumps; taking in a rescan without moving any game (new games go on the end, gone ones are marked). |
| `rescan.rs` | A thread watching each console's folders and the cards (inotify): a console whose folder changed is scanned again once quiet for a second, and the shelf takes in what it found between frames. Menu → System → Rescan games looks at every console. |
| `emulator.rs` | Emulator descriptions, as nibkit's `dslot` module reads them: which are installed, each console's default, their declared options and actions. |
| `launch.rs` | Choosing the emulator for a game, its options from the settings, its command with the placeholders filled in, running it; actions on a game's files. |
| `settings/` | DSlot's settings on nibkit's layered store (`mod.rs`: the page, typed accessors), DSlot's own settings (`def.rs`), reading older versions' files once (`migrate.rs`). docs/settings.md. |
| `stats.rs` | Play time, favourites and ratings per game. |
| `kms.rs` | DRM ioctls for the skeleton: dumb buffers set on the CRTCs the console lights. |
| `splash.rs` | nibOS's power-on picture, written once when the package is built. |
| `ui/screens.rs` | SDL, the windows and renderers where nibkit places the screens: one per panel, one spanning both, or the desktop preview's stacked pair. |
| `ui/shelf.rs` | The frame loop and the shelf's `State`: input, touches, the carts' motion, the slot, artwork wanted, drawing. |
| `ui/input.rs` | SDL's events as the DS+'s buttons by their printed labels, the keyboard's stand-ins, the mouse as the touch screen. |
| `ui/menu.rs` | The menu's pages (Game, console, System), their rows, where each value comes from, and what each row does. |
| `ui/game.rs` | The game page (X): its buttons and the game's achievement badges on the touch screen, and moving between them. |
| `ui/back.rs` | The back of the game's box, on the top screen while its page is open: screenshots, what it is, the player's play, a barcode, the chosen achievement as a sticker. |
| `details.rs` | A game's details from the web for the box's back: libretro's thumbnails, Wikipedia and Wikidata, by `curl` on a thread, kept in the cache. |
| `ui/sheet.rs` | The View sheet (Start): the menu's View page alone, drawn on the top screen so the shelf it changes stays in sight. |
| `ui/draw.rs` | The textures and how each part of the screens is drawn: the shelf, grid and lists, the top screen (with the resume inset), the system picker, the cards, the status bar (battery, Wi-Fi, Bluetooth from `nibkit::net`, the clock). |
| `ui/layout.rs` | Where things are, in units of a 1024 × 768 screen, and hit tests. |
| `ui/skeleton.rs`, `ui/sound.rs` | The empty shelf for start-up; the insert and eject clicks (through `pacat`, or SDL). |

## Platforms

`src/platform/mod.rs` picks `stock` or `nibos` by Cargo feature (exactly one; anything else is
a compile error). Each provides the same constants and functions, so the rest of DSlot uses
`platform::X` without asking which it is:

| | Stock OS | nibOS |
|---|---|---|
| Games | `/mnt/sdcard/Roms`, `/mnt/mmc/Roms` | `/mnt/roms/Roms`, `/mnt/ds/Roms` |
| DSlot's files | beside the binary, on the card | XDG folders in the user's home |
| Saves | the emulators' own | `/mnt/roms/Saves/<system>/<name>/` |
| Displays | one spanning both panels (Weston), touch on the right | one per panel (KMS), touch on the left |
| Face buttons | SDL's names by position on an Xbox pad | A and B by position, X and Y as printed |
| Menu button | leaves DSlot | does nothing (it brings you back from games) |
| Start-up | — | splash or skeleton, Mesa preloaded |
| Start at power-on | the stock boot script's `update.sh` hook, from the menu | `nib boot-ui`, not in the menu |
| Power off | `sync; poweroff` | `systemctl poweroff` |
| Device settings | — (the stock helper keeps them) | volume (`pactl`), each screen's brightness (`/run/nibos/backlight`), Wi-Fi (`nib wifi`) |
| Status bar | the clock | the battery, Wi-Fi and Bluetooth (`nibkit::net`), the clock |
| Apps | `Roms/APPS` | `Roms/APPS`, and packages' in `/usr/share/dslot/apps` |
| Packaging | `packaging/stock-card` (zip) | `packaging/nibos` (PKGBUILD, unit) |

## Choices worth knowing

- **Everything drawn in software, once.** Carts and boxes are composed into images on worker
  threads and kept as textures; a frame only copies textures. Grid covers are cached on disk
  already scaled.
- **`unsafe_textures`.** SDL2's textures without lifetimes, so the shelf's state can hold them;
  DSlot destroys each one itself (`ui/draw.rs`, `destroy`).
- **Sound through `pacat`.** The stock OS's SDL reaches PulseAudio through an ALSA shim that
  underruns on small buffers; `pacat` takes the whole clip and lets it finish after the shelf has
  gone.
- **The touch panel from evdev.** Read directly, it gives real touches on both systems; SDL's
  mouse stands in only in the desktop preview.
- **A texture belongs to one renderer.** On nibOS each screen is a window with a renderer of
  its own (`ui/screens.rs`), and SDL refuses to draw a texture on any renderer but the one that
  made it ("Texture was not created with this renderer"). Every texture is made for the screen
  it is drawn on: `white`, `star`, `thumbs` and the logo are one per canvas, indexed by
  `View::c`; a game's box art is made for the top screen and again for the bottom one where the
  system picker draws it (`Art::blade`). The single-window desktop preview can't show the
  mistake, so `DSLOT_TWO_WINDOWS=1` gives a headless run nibOS's two windows, and
  `tests/picker.rs` opens the picker from every page that way. (DSlot exited on Select until
  this was fixed.) A picker or card that still fails to draw is logged once and left out of
  the frame, not fatal.
- **Resting.** The frame loop draws and presents only while something can change: for three
  seconds after the last input, while a cart moves, a finger is down, a direction is held,
  games are found, or artwork or a resume picture is coming in. Otherwise it polls input every
  50 ms, and draws when the status bar's clock, battery or radios change (`Ui::status`). An
  unchanging shelf otherwise kept the GPU at 60 frames a second.
- **Work off the frame.** Artwork is composed on the loader's two workers. Resume pictures are
  read on a worker of their own that answers only the latest request, so browsing quickly never
  queues reads. The radios are read every few seconds on nibkit's monitor thread. Folders are
  watched by `rescan`'s thread.
- **Emulators described, not hard-wired.** A new emulator, or another way of starting one, is a
  file, not a change to DSlot (docs/emulators.md).
