# DSlot

A game shelf for the Anbernic RG DS+. The touch screen holds your carts over a card slot; the top screen shows the selected game's box, how long you have played it and your rating. Push a cart into the slot to play.

![The shelf: a game's box on the top screen, carts over the slot on the touch screen](docs/shelf.png)

- Carts and boxes for every game, from your own artwork or made from what the ROM carries
- A page for each system with an emulator installed and games on a card: DS, GBA, Game Boy, Game Boy Color, SNES, NES, Famicom Disk System, Mega Drive, Master System, SG-1000, Game Gear, PC Engine, PC Engine CD, PlayStation, Atari Lynx, Neo Geo Pocket, WonderSwan and Game & Watch, and PortMaster's ports
- A system picker on Select, and a Top Trumps style card for each system, with its photo and cited facts; the Apps page's card is about the device itself
- Where you left off: a game's resume point, as its emulator saved it, inset over its box
- A cover grid or a list of names (with or without small boxes) as alternatives to the carts, all scrolling with momentum under a finger
- Games copied on or deleted show up or go by themselves
- Favourites, star ratings, play time, and sorting by name, recently played or rating
- The battery, Wi-Fi and Bluetooth in the status bar
- Rests when left alone: no frames drawn while nothing changes
- The emulator for each system chosen from the ones installed, and per game if you like
- Insert and eject sounds recorded from a real card slot
- Goes straight back to the shelf when you leave a game
- Can start at power-on, and can power the device off

Inspired by [slot.](https://github.com/BrandonKowalski/slot) Written in Rust on SDL2.

| Cover grid | Menu |
|---|---|
| ![Cover grid](docs/grid.png) | ![Menu](docs/menu.png) |

The screenshots use made-up games. With your own box art in place it looks like your shelf.

On the DS+'s stock OS? See [On the stock OS](#on-the-stock-os): until a stock-OS zip of this
version is out, the ready-made one is the [DSlot 0.2.0 release](https://github.com/abradburne/DSlot/releases/tag/v0.2.0).

## Two builds

DSlot is one program built for one of two systems:

- **The stock OS**, the system the DS+ comes with. DSlot is an app in the stock menu's Apps,
  installed on the SD card; nothing on the system partitions is changed. Games start in the
  emulators the stock OS has: NNDDSS for DS (as from the stock menu), RetroArch's mGBA for GBA.
- **nibOS**, a mainline Linux and Arch Linux ARM system for small handhelds, made alongside
  DSlot. There DSlot is the boot UI: it is on both screens about five seconds after the
  power button, in place of a login. Games start in [DSperate](https://github.com/abradburne/DSperate), DS and GBA.

What is the same on both: everything you see. What differs (where games and DSlot's files are,
how it starts at power-on, how it gets the screens) is in [`src/platform/`](src/platform), and
[docs/building.md](docs/building.md) builds either.

## On the stock OS

> **Looking for DSlot for the stock OS?** This version builds for it, but its stock-OS zip is
> not out yet. Until it is, install the ready-made zip from the
> [DSlot 0.2.0 release](https://github.com/abradburne/DSlot/releases/tag/v0.2.0), the last
> stock-OS-only version. Its source is on the [`stock-0.2`](https://github.com/abradburne/DSlot/tree/stock-0.2) branch.

### What you need

- An Anbernic RG DS+ on the stock OS. DSlot was made and tested on firmware V1.0 (20260915).
- Games in `Roms/NDS` and `Roms/GBA` on either SD card, where the stock menu already expects them.
- A computer with an SD card reader.

### Install

1. Download `DSlot-<version>-rgdsplus.zip` from the [latest release](https://github.com/abradburne/DSlot/releases/latest) and unzip it. Inside is a folder, `Roms`, holding one folder, `APPS`.
2. Turn the DS+ off, take out the SD card in the slot marked **TF1/INT** (the one the DS+ starts from) and put it in your computer. It shows as a drive called `ROMS`. Install DSlot there rather than on the games card in TF2: that card is usually exFAT, which the DS+ reads much more slowly, and only from TF1 can DSlot start early with [dsboot](https://github.com/abradburne/dsboot). DSlot finds games on both cards wherever it is installed.
3. On the card, open `Roms`, then `APPS`. (No `APPS` folder there? Create one, in capitals.)
4. Copy **everything inside the zip's `Roms/APPS` folder** into the card's `Roms/APPS` folder. That is four things:

   ```
   dslot/               folder
   DSlot.sh
   DSlot_boot_on.sh
   DSlot_boot_off.sh
   ```

   Copy the contents, not the `Roms` folder itself: dragging `Roms` onto the card makes some computers replace the card's `Roms` folder, games and all, or put a second one (`Roms 2`) beside it.
5. Eject the card, put it back in the DS+ and turn it on.
6. In the stock menu open **Apps** and choose **DSlot**.

When it is right, the card has `Roms/APPS/DSlot.sh` and `Roms/APPS/dslot/dslot` on it.

The first start reads every game and prepares its artwork, so covers may take a moment to appear. After that they are kept ready.

### Start at power-on

In DSlot press **X** for the menu, go to **Start at power-on** and press **A** to turn it on. The DS+ then boots into DSlot. Leaving DSlot (Select + Start, or Menu) takes you to the stock menu.

If you would rather do it from the stock menu, **DSlot_boot_on** and **DSlot_boot_off** in Apps do the same.

How it works: it puts one file on the card, `update/update.sh`, which the stock boot script runs before its own menu comes up. To undo it without the device, delete that file from the card on a computer.

On the card the DS+ starts from, it also puts `update/early.sh` beside it. The stock system never runs that file. [dsboot](https://github.com/abradburne/dsboot), a faster start-up for the stock OS, runs it as soon as the screen is up, a second or more before the stock boot script gets to `update.sh`; DSlot then appears that much sooner, and `update.sh` only waits for it. Delete both files to undo it by hand.

### Update

Do the install again with the new zip, replacing the files on the card. Your play times, favourites, ratings and settings are kept (0.2's `settings.ini` is read into the new `settings.toml` once).

From 0.1.0: first delete `dslot.sh`, `dslot_boot_on.sh` and `dslot_boot_off.sh` from the card's `Roms/APPS`. They are now `DSlot.sh` and so on, and copying over files whose names differ only in capitals keeps the old names. Leave the `dslot` folder: it holds your play times and settings.

### Remove

Turn off **Start at power-on**, then delete `dslot`, `DSlot.sh`, `DSlot_boot_on.sh` and `DSlot_boot_off.sh` from the card's `Roms/APPS`. (Already removed it without turning that off? Delete `update/update.sh` and `update/early.sh` from the card as well.)

## On nibOS

DSlot comes with nibOS, as the `dslot` package; `sudo nib boot-ui on` makes it what the DS+
starts into, `off` puts the login shell back. Games are read from `Roms/` on the card nibOS
starts from (its ROMS partition, `/mnt/roms`) and on the card in TF2 (`/mnt/ds`). Leaving DSlot
(Select + Start) starts the shell on tty1. How it is on screen so soon:
[docs/nibos.md](docs/nibos.md).

## Pages

The shelf has a page for each system that has an installed emulator and games on a card (Menu
→ System → **Show systems with no games** shows the empty ones too). L / R step through the
pages (round from the last to the first), or tap a page's tab at the top of the touch screen;
when there are more tabs than fit, the strip scrolls to keep the page's tab in view, and the
faded ends can be tapped. **Select** opens the system picker: a stack of blades, one per page,
each in its system's colour with box art from one of its games, the chosen one showing how many
games it has and when one was last played, with the system's card on the top screen. Up / down
(or a drag) chooses, A or a tap goes there, B or Select closes it. The page is remembered, and
coming back from a game opens the page it was on.

Everything else is the same shelf on every page: the cover grid and the lists, sorting, favourites, ratings
and play time. DS games are `.nds` files (or `.zip`, listed by file name) in `Roms/NDS`; their
title, icon and game code come from the ROM. GBA games are `.gba` files, or `.zip` files holding
one, in `Roms/GBA`; the name shown is the file's, less what is in brackets (`Advance Wars 2 -
Black Hole Rising (USA)` shows as `Advance Wars 2 - Black Hole Rising`), and the game code and
maker come from the cartridge header. A GBA cart is drawn after a real one, and goes into a DS
Lite's slot-2.

SNES games are `.sfc`, `.smc` or `.zip` files in `Roms/SFC` on either card. The **SNES** page
has grey Super Famicom-style carts and landscape boxes, with the usual `Imgs`, `Labels` and
`Carts` artwork overrides. Names come from the file; available game codes and publishers come
from the cartridge header. On nibOS an installed emulator must describe the `SFC` system;
installing nibcase and a SNES core registers it and its options automatically. DSlot refreshes
its pages and emulator menus when integrations change. The stock build ships a
Snes9x/RetroArch description. See [SNES support](docs/snes.md).

NES cartridges use `Roms/FC` (`.nes`, `.unf`, `.unif`, or a single-game `.zip`), with a grey
front-loading cartridge and portrait boxes. Installing nibcase and its Nestopia package adds
the page and core options automatically. See [NES support](docs/nes.md).

Mega Drive, Game Gear and Atari Lynx use `Roms/MD`, `Roms/GG` and `Roms/LYNX`. Install
`nibcase-blastem`, `nibcase-gearsystem` or `nibcase-handy` on nibOS to register the core and its
options automatically. Lynx also needs your `lynxboot.img` in a card's `BIOS/` folder. See
[Sega and Lynx support](docs/sega-lynx.md).

Game & Watch units (`.mgw`) go in `Roms/GW`, named after their files. They have no carts, so
their page shows boxes, as a grid or a list; `nibcase-gw` plays them, two-screen units across
both panels. See [Game & Watch support](docs/game-watch.md).

The Master System, SG-1000, PC Engine, PC Engine CD, PlayStation, Neo Geo Pocket, WonderSwan and Famicom Disk System
have pages too, with boxes for now (no carts drawn for them yet). See
[More systems](docs/more-systems.md).

Games copied onto a card or deleted from it while DSlot is running show up or disappear on
their page about a second after the copy finishes, and so do a card's games when it is put in
or taken out; the selection stays where it was. **Rescan games** on the menu's System page
looks again at every page, for a change DSlot didn't notice.

PortMaster's ports in `Roms/PORTS` have a **Ports** page, each named and pictured from its own
folder and started with PortMaster's runner; while one plays on the top screen, the bottom one
says how to quit it. See [Ports](docs/ports.md).

## The top screen

The selected game's box, its name, play time, when it was last played, sessions and rating.
When the game has a **resume point** (its emulator saved where you left off when you last
quit), its picture is inset over the box's bottom right corner, framed, with a "Resume" tag and
when it was saved: for a DS game both screens, one over the other. It is the resume point the
next start will use, the chosen emulator's own; one saved without a picture isn't shown. How
DSlot reads each emulator's: [docs/emulators.md](docs/emulators.md#resume-points).

In the system picker, and on the menu's console page, the top screen shows the **system's
card**: a Top Trumps style card with its maker, stats (released, launch price, sold,
processor, memory, screen, games) with bars comparing the systems, a short history, a fun
fact, a photo of the console, and what is on your cards for it. Every figure is from the cited
source printed on the card; the photos are public domain or Creative Commons, credited on each
card and in `photo-credits.md`. The Apps page's card is about the device you are holding: its
chip, memory and screens, its system and build date, the battery (and battery protect), the
room on each card, how long it has been on, the time played in all and the game played most.
How cards are made, and how to add a system: [docs/system-cards.md](docs/system-cards.md).

On a game's page (X), the top screen is the **back of its box**: its title screen and a
screenshot in play, the year, developer, publisher and genre, a blurb, your play time, sessions
and achievements, and a barcode; the achievement badge chosen on the touch screen goes on it as
a sticker. The details come from the web when the page is opened, and only then, by `curl`:
the screenshots from libretro's thumbnails (thumbnails.libretro.com, by the ROM's file name,
so a ROM named as No-Intro names it finds them), what the game is from Wikipedia's search and
summary (en.wikipedia.org, the blurb credited on the box, CC BY-SA) and Wikidata
(www.wikidata.org). A game with no box art of its own gets its box art from libretro's
thumbnails then too (see Artwork). Nothing else is sent: the game's name and system, and
DSlot's version in the User-Agent. They are kept in the cache, `details/<system>/<ROM name>/`, and asked for again
after 90 days (a week for what wasn't found); with no network the page shows what was kept.
**Y** on the page asks for them all again now, for a lookup that found nothing or the wrong
game: a note on the box says so while it runs and what it found after. What the servers answer
replaces what was kept; with no network, what was kept stays.

## The status bar

At the bottom of the touch screen: the battery (charge, and a bolt or plug), Wi-Fi (off,
connecting, or its signal in bars) and Bluetooth (off, on, or connected, with how many devices when more than one),
and the clock (12- or 24-hour, as nibOS's preference says). The stock OS keeps its radios to
itself, so there it shows the battery and the clock.

On nibOS, a tap on the Wi-Fi or Bluetooth icon opens its page in the menu.

## Wi-Fi and Bluetooth

On nibOS the menu (X) has a **Wi-Fi** page and a **Bluetooth** page beside System.

- **Wi-Fi:** the switch, the network joined, Look for networks, then each network in range with
  its signal and whether it is saved. A on a network joins it; one that needs a passphrase
  brings up the keyboard. A twice on the joined one leaves it; Y twice on a saved one forgets
  it.
- **Bluetooth:** the switch, the paired devices (headphones, gamepads...) with their battery
  where they say: A connects or disconnects one, Y twice forgets it. **Pair a new device**
  looks for devices (put yours in pairing mode); A on one pairs it, asking for a PIN, or to
  check a passkey on both, where it needs one, and connects it.
- **The keyboard:** the D-pad and A type (or tap the keys), B deletes, X a space, Y shows a
  passphrase, L shift, R symbols, Start is Done, Select cancels.

Joining a network from a file on the card (`ROMS/nibos/wifi/<network>.psk` on the ROMS volume, nibOS's way) still works.

## Resting

Three seconds after you last touch anything, with nothing moving or loading, DSlot stops
drawing frames: the screens keep what they show, and DSlot only looks for input, 20 times a
second, until a button or a touch, or a change in the status bar, wants a new frame. Left long
enough, the screensaver (Menu → System) dims the screens and bounces DSlot's logo.

## Apps

Programs in `Roms/APPS` on either card (and, on nibOS, those a package installs in
`/usr/share/dslot/apps`) that describe themselves in an `app.toml` get an
**Apps** tab beside the consoles, a grid of their icons (or a list, in the list views); on the stock OS, the stock menu's app
scripts too. How to make one: [docs/apps.md](docs/apps.md).

## Controls

| | |
|---|---|
| D-pad left / right, swipe | Browse (in a list: jump to the previous / next letter) |
| D-pad up / down | Jump to the previous / next letter (a row, in the cover grid; a game, in a list) |
| L / R, or tap a page's tab | The previous / next page (the tabs scroll when there are more than fit; tap a faded end to see more) |
| Select | The system picker: a blade for each system, its games and when one was last played, and its card on the top screen. Up / down (or drag) to choose, A or a tap to go there, B or Select to close |
| A, or drag the cart down | Insert the cart and play |
| Tap a neighbouring cart | Bring it to the centre |
| Tap a cover in the grid or a name in a list, then tap it again | Pick it, then play it |
| Flick the grid or a list | It keeps scrolling and slows to a stop, as on a phone; touch it to stop it |
| Y | Mark or unmark the game as a favourite |
| Back (beside Menu) | Open the menu |
| X | The game page: the box turns round to its back on the top screen (screenshots, what the game is, your play); Play, Favourite, Rating, Settings and the game's achievement badges on the touch screen. The D-pad moves, L / R step along the buttons (as they turn the menu's tabs), A (or a tap) presses or picks a badge, Y looks the game's screenshots, box art and details up again, B or X closes it, Back opens the menu over it |
| Start | The View sheet: how this page is shown (View, Sort by, Show, Game case) on the top screen, the shelf below changing as you go. Up / down picks a row, left / right changes it, Y goes back to what it inherits, Start, B or a touch closes it |
| Select + Start | Leave DSlot (on the stock OS, Menu too) |
| Hold power (nibOS) | The power menu: Sleep, Restart, Power off, Cancel. During a game, the game is saved and put away first; Cancel carries on with it |

On nibOS Menu does nothing on the shelf: it is the button that brings you back from a game.

## Menu

Back opens the menu on the touch screen, with a page for each level of settings: **Game** (the
selected game), its **console** (DS, GBA, ...) and **System**, and on nibOS **Network** (Wi-Fi
and Bluetooth) and **Achievements**. A page with more than a screen's worth is in sections, in
a row under the tabs. L / R or a tap on a tab changes page, L2 / R2 or a tap on a section
changes section; D-pad up / down picks a row, left / right (or A, or a tap on the row) changes
it, Y resets it to what it inherits, B or Back closes the menu.

| Page | Sections | |
|---|---|---|
| Game | | Favourite and rating; the emulator it starts in and that emulator's options for it (DSperate: 3D resolution, frame skip, ...); clear its resume state, delete its saves (each asks first: A, then A again). |
| Console | Emulator, Shelf | The emulator its games start in (only when more than one is installed) and that emulator's options; how the page is shown: View, Sort by, Show, Game case. |
| System | Shelf, Screen, Sound, Power, Time, About, Update | How every page is shown (unless its console says otherwise), Button repeat, Rescan games; each screen's brightness and the screensaver; the volume, Sounds and the speakers' tuning; the battery, Start at power-on, Power off; the clock; the device and what it is using; on nibOS, its updates. |
| Network | Wi-Fi, Bluetooth, Network info | nibOS: the networks and devices, and what to read out when something won't connect. |

A setting for a game wins over its console's, which wins over the system's, and each row says
where its value comes from. All of it: [docs/settings.md](docs/settings.md).

## Emulators

Each emulator DSlot can use describes itself in a small file: which consoles it plays, how to
start a game in it, which of its options DSlot may show, and what can be done to a game's
files. A console's games start in the installed one that ranks highest (DSperate, where it is
installed), or in the one picked for the console or the game in the menu. How to write one:
[docs/emulators.md](docs/emulators.md).

## Where DSlot keeps things

| | Stock OS | nibOS |
|---|---|---|
| Play times, favourites, ratings (`stats.tsv`) | `Roms/APPS/dslot/` | `~/.local/share/dslot/` |
| Settings, for the system, each console and each game (`settings.toml`) | `Roms/APPS/dslot/` | `~/.config/dslot/` |
| Emulator descriptions | `Roms/APPS/dslot/emulators/` | `/usr/share/dslot/emulators/`, `/etc/dslot/emulators/`, `~/.config/dslot/emulators/` |
| Cache: what each ROM said, grid covers, the game pages' details and screenshots | `Roms/APPS/dslot/cache/` | `~/.cache/dslot/` |
| Game saves, and resume points | the emulators' own | `/mnt/roms/Saves/<system>/<ROM name>/` |
| Apps installed with the system | — | `/usr/share/dslot/apps/` |
| The system cards' photo credits | — | `/usr/share/doc/dslot/photo-credits.md` |

**Game folders.** DSlot makes the folder of every system an installed emulator plays, and
PortMaster's `PORTS`, where a card is missing one: at start, after an emulator is installed,
and on Menu → System → Rescan games. Only on a card already laid out for games (a `Roms`
folder, or nibOS's own data card, `/mnt/roms`); a card set up for slot CFW gets slot's
`Games/GBA`, `Games/GB` and `Games/GBC` too. Each folder it makes has a `_README.txt`: the
files it takes, the BIOS it needs and where saves go. Nothing is ever renamed, moved or
deleted, and a card with neither folder is left alone. An empty system still has no page.

The cache can be deleted at any time; it is rebuilt as needed. Artwork is read and scaled on
background threads, nearest the selection first, so browsing never waits for it, and grid
covers are kept there already scaled (about 120 KB a game), which makes every start after the
first quick. A ROM is known there by its file name: one replaced by a different game under the
same name keeps the old title and icon until the cache is deleted.

## Artwork

Artwork is optional. With none, every cart gets a sticker printed from the ROM's own title,
icon and game code, and every box is drawn from the same. Box art goes in `Imgs/` beside the
ROMs, the folder the stock menu uses, so one set of covers serves both; labels and finished
carts can go in `Labels/` and `Carts/`. Names, shapes and sizes: [docs/artwork.md](docs/artwork.md).

A game with no box art in `Imgs/` gets libretro's (thumbnails.libretro.com, by the ROM's file
name) the first time its page (X) is opened with a network: it is kept in DSlot's cache, not on
the card, and used wherever the card has none. Art you put in `Imgs/` always comes first.

A card set up for slot CFW (`Games/GBA`, `Labels/GBA`, Cart Studio's `cart_shell.txt`) works
as it is, beside the stock layout: [docs/slot-cards.md](docs/slot-cards.md).

## Troubleshooting

- **DSlot is not in Apps** (stock OS). The files are in the wrong place. Check the card has `Roms/APPS/DSlot.sh` (not `Roms/APPS/Roms/APPS/DSlot.sh`, and not in a `Roms 2` folder).
- **No games listed.** DSlot reads `Roms/NDS` and `Roms/GBA` on both cards; the empty page lists the folders it looked in.
- **A game does not start.** When a game comes straight back to the shelf (the cart goes in, the screen goes dark, the shelf is back), the shelf says why in a box on the touch screen: the emulator, how it stopped (its exit status or signal) and the last thing it said, e.g. "DSperate stopped at once (exit 1): launch: Game.nds cannot start: ZIP NOT SUPPORTED"; any button closes it. On nibOS the notice also offers **Y: send a bug report** (`nib bug-report send`, docs/nibos.md): it saves a scrubbed copy of this start's log on the card as `nibos/bug-report-DATE.txt` and sends it to the bug tracker, then shows a report code to quote, or says that the report could not be sent and is on the card. The same line is logged: `Roms/APPS/dslot/log.txt` on the stock OS, `journalctl -u dslot` on nibOS, with everything the emulator wrote before it. "No emulator for GBA games here" means no installed emulator's description lists that console (docs/emulators.md). `dslot --launch ROM` starts one game as the shelf would, and prints the command.
- **The two screens are the wrong way round.** Stock OS: edit `Roms/APPS/DSlot.sh` and add `export DSLOT_SWAP=1` above the last line. nibOS: `DSLOT_SWAP=1` in `/etc/default/dslot`.
- **Touch does nothing or lands in the wrong place.** Buttons work regardless. Set `DSLOT_TOUCH_DEV=/dev/input/eventN` the same way, with the touch panel's event number.
- **It does not start at power-on** (stock OS). That relies on the stock `/mnt/mod/ctrl/autostart` script, which is part of firmware V1.0 (20260915). Other firmware may not have it.

On the stock OS, `Roms/APPS/dslot/log.txt` and `diag.txt` record what DSlot saw the last time it started, and how long each stage of starting took. Attach both to a bug report.

## For developers

[docs/architecture.md](docs/architecture.md) is the map of the code, [docs/building.md](docs/building.md)
how to build, test and package both builds and run the desktop preview, and
[CHANGELOG.md](CHANGELOG.md) what changed. Contributing: [CONTRIBUTING.md](CONTRIBUTING.md).

## Licence

GPL-3.0-or-later; see [LICENSE](LICENSE).
The crates built into DSlot keep their own licences, all compatible with it: they are listed,
with their licence texts, in [THIRD-PARTY.md](THIRD-PARTY.md), and checked in
[docs/licences.md](docs/licences.md). nibkit, the nibOS SDK DSlot builds on, is MIT.

- Roboto Condensed, embedded for the text, is under the SIL Open Font License 1.1
  ([assets/OFL.txt](assets/OFL.txt)); copyright 2011 The Roboto Project Authors.
- The insert and eject sounds are our own recording of a DS card slot.
- DSlot contains no game artwork, ROMs, BIOS files or code from Nintendo or any emulator. Game titles and icons on screen are read from your own ROM files as it runs.

DSlot is not affiliated with Nintendo or Anbernic. Nintendo DS is a trademark of Nintendo.
