# Settings

DSlot's settings are kept at three levels:

| Level | What it holds | Menu page |
|---|---|---|
| **System** | DSlot and the device as a whole | System |
| **Console** | Every game of a console: DS, GBA, GB, GBC | the console's (DS, GBA, ...) |
| **Game** | One game | Game |

A setting set for a game wins over its console's, which wins over the system's; one set nowhere
is its default. Each setting can be set at some of the levels:

| Setting | System | Console | Game | Default |
|---|---|---|---|---|
| View (carts, cover grid, list, or list with covers) | ✓ | ✓ | | Carts |
| Sort by | ✓ | ✓ | | Name |
| Show (all games or favourites) | ✓ | ✓ | | All games |
| Game case | ✓ | ✓ | | By region |
| SNES carts (match the game's region, US, or Super Famicom; the SNES page only) | | ✓ | | Match the game's region |
| Show systems with no games (a page for each system with an emulator, games or not) | ✓ | | | Off |
| Sounds (the slot's clicks) | ✓ | | | On |
| Button repeat (a held D-pad) | ✓ | | | Normal |
| Screensaver (off, or after 1, 2, 5 or 10 minutes left alone) | ✓ | | | 2 min |
| Screensaver style (Animated: the logo drifting, drawn a few times a second; Still: moved once a minute; Screens off: the backlights off) | ✓ | | | Animated |
| Mode (RetroAchievements: softcore or hardcore; the Achievements page, docs/retroachievements.md; hidden until hardcore is offered) | ✓ | | | Softcore |
| Emulator | | ✓ | ✓ | the console's highest-priority one |
| An emulator's options (docs/emulators.md) | | ✓ | ✓ | the emulator's own (not passed) |

On a console's page and a game's, each row's values start with what it inherits: *Use System
(Carts)*, *Use GBA (Light)*, *Use default (Name)*, or for an emulator's option set nowhere *Use
DSperate's own* (not passed at all). Inheriting, the row shows that, muted; set on this page, it
shows its value, white. Stepping back round to *Use ...* clears it here, and so does **Y**,
which the footer offers (*Y use GBA*) on a row set here. The System page has no one to inherit
from: its rows are values.

Not kept in DSlot's file, but on the System page: the device's **volume**, the **brightness** of
each screen, **Wi-Fi**, the **battery** and **battery protect**, and the **clock** (12-hour or
24-hour, nibOS's preference for every app: `nibkit::clock`, else the locale's habit), where the platform lets DSlot change them (nibOS; on the stock OS
its own helper keeps volume and brightness, and DSlot leaves them be). Brightness changes as
the slider moves, and is saved by nibOS when it is let go of; the sliders follow changes made
with Menu + volume while the page is shown (DSlot keeps no level of its own). Wi-Fi on nibOS needs a polkit
rule letting DSlot's user start and stop the network; without one the row shows the state
and the change is logged as refused. **Rescan games** (games copied or deleted while DSlot
runs are noticed by themselves; this looks again at every page), **Start at power-on** and
**Power off** are there too.

**System info** (its row, or a tap on it) is a read-only page of the device and what it is
using, for a player asked "what does it say under System info?" and for anyone chasing memory
on a 1 GB device: the board and the nibOS release (version, edition, channel; its build as a
note), the kernel, how long it has been up, the A/B slot running (from the root partition's
label on the kernel's command line), DSlot's version with nibkit's; the memory in use of the
total (a bar, as `/proc/meminfo` has it), what is available, DSlot's own resident size, the
GPU's shared memory (`Shmem`), the file cache, the swap in use (on zram) and the kernel's CMA
pool; the load averages, each core's frequency with the governor, the temperatures of the
thermal zones, the GPU's frequency; the system, NIBOS-DATA and TF2 card file systems (the
card only while one is mounted); the battery; and each panel's mode. Read from `/proc` and
`/sys` on a thread every second while the page is up (`src/ui/sysinfo.rs`); nothing is run,
nothing needs the network, and a line whose source the device lacks (a desktop preview, the
stock OS) is left out rather than guessed. **B** goes back to the System page.

## Screensaver

Left alone on the shelf (or its menu) for the Screensaver setting's time, DSlot dims both
screens and bounces its logo round them, the two screens one tall space with the hinge between
them: the logo crosses it out of sight, changes colour at each wall, and turns half way round
the colour wheel when it meets a corner. Any button, touch or the lid wakes it, and that first
press does nothing else. The screens go back to exactly the brightness they had; the dimming is
never saved as the brightness (nibkit's `backlight::dim`). It draws about 30 frames a second
and nothing else of the shelf. On the stock OS, which keeps brightness to itself, the logo
bounces over black at the screens' brightness.

The Game page also has the selected game's favourite and rating (kept with its play time in
`stats.tsv`) and actions on its files: an emulator's own (DSperate's *Clear resume state*) and
*Delete saves*. Each asks first: A, then A again.

## settings.toml

All the levels are in one file, `settings.toml`: `Roms/APPS/dslot/` on the stock OS,
`~/.config/dslot/` on nibOS. DSlot writes it when a setting changes in its menu; it can be
edited by hand while DSlot is not running.

```toml
[state]
page = "gba"                 # the shelf's page

[system]
view = "grid"
sort = "recent"
sounds = true

[console.GBA]
view = "shelf"               # the GBA page in carts, the others in the grid

[console.SFC]
view = "list"                # names only ("list-covers": a small box beside each)
emulator = "mgba"

[console.NDS.dsperate]       # DSperate's options for DS games
hires3d = 2
args = "--speed 100"         # extra words, for options DSperate doesn't declare

[game."NDS/Golden Sun - Dark Dawn (USA)".dsperate]
hires3d = 1
```

A game is known by its console's folder and its ROM's file name without the extension. Values
a setting doesn't take, and sections DSlot doesn't know, are skipped.

An emulator with a menu of its own may keep the options it declares here too, under its id, so
both menus show the same values (`nibkit::dslot::settings_file`). DSlot reads the file again
each time a game exits, and doesn't write it while one runs. The file is always replaced whole,
never written in place.

## From older versions

The first time DSlot finds no settings.toml, it reads what older versions kept: DSlot 0.2's
`settings.ini` (beside the program on the stock OS) and the first 0.3 builds' `settings.ini` and
`launch.conf` (docs/emulators.md, "launch.conf"). They are then renamed `*.old`.
