# Emulators

DSlot starts games itself: it picks the emulator, puts together its command line, makes the
game's saves folder where it decides that, and waits for the emulator to exit. Emulators tell
it what they play, how to start them, which of their options DSlot may show in its settings,
and what can be done to a game's files, in a description file each. Which emulator, and the
options' values, are DSlot's settings (docs/settings.md), for a console or a single game.

## Description files

One TOML file per emulator. Its name, without `.toml`, is the emulator's id: what
`settings.ini` and `launch.conf` call it. DSlot reads every `*.toml` in these folders, in this
order; a file replaces one of the same name in an earlier folder, so a user can override a
package's description by copying it:

| Stock OS | nibOS |
|---|---|
| `Roms/APPS/dslot/emulators/` (shipped with DSlot) | `/usr/share/dslot/emulators/` (emulators' packages) |
| | `/etc/dslot/emulators/` (the administrator's) |
| | `~/.config/dslot/emulators/` (the user's) |

DSperate's, which its nibOS package installs (shortened):

```toml
name = "DSperate"
priority = 100

# Each system it plays and the command for it.
[systems]
NDS = ["/usr/bin/dsperate", "{options}", "--saves", "{saves}", "--states", "{saves}", "{rom}"]
GBA = ["/usr/bin/dsperate", "--gba", "{options}", "--saves", "{saves}", "--states", "{saves}", "{rom}"]

[env]
DS_DEBUG = "marks"

# Its settings for the handheld, put in place before the first game when there are none.
[[seed]]
from = "/usr/share/dsperate/dslot/dsperate.ini"
to = "~/.config/dsperate/dsperate.ini"

# An option DSlot shows, for a console and per game.
[[options]]
key = "hires3d"
label = "3D resolution"
systems = ["NDS"]
values = [1, 2, 3, 4]
labels = ["1x (256x192)", "2x (512x384)", "3x (768x576)", "4x (1024x768, slow)"]
default = 1
args = ["--hires3d", "{value}"]

[[options]]
key = "aa"
label = "3D anti-aliasing"
systems = ["NDS"]
kind = "toggle"
args = ["--aa"]
off = ["--no-aa"]

# An action on a game's files, on the Game page of the menu (A, then A again).
[[actions]]
label = "Clear resume state"
delete = ["{saves}/*.auto.dss"]
```

| Key | | |
|---|---|---|
| `name` | required | Shown in the menu's Emulator row. |
| `priority` | default 0 | Each system's default is the installed emulator with the highest. The stock descriptions use 10 and 20; DSperate, 100. |
| `[systems]` | required | A command for each system it plays, by the system's `Roms/` folder name: `NDS`, `GBA`, `GB`, `GBC`, `SFC` (SNES), `FC` (NES), `FDS`, `MD`, `SMS`, `SG1000`, `GG`, `PCE`, `PCECD`, `PS`, `LYNX`, `NGP`, `WS`, `GW` (Game & Watch). A system DSlot does not know is skipped. |
| `requires` | optional | Files that must be there as well as the programs: `["/path/to/core.so"]`. |
| `[env]` | optional | Environment variables for the emulator. When the player is signed in to RetroAchievements, DSlot adds `RA_TOKEN_FILE`, `RA_USERNAME` and `RA_HARDCORE` to them (docs/retroachievements.md). |
| `[[seed]]` | optional | Files copied into place before a game starts, when there is none there yet: `from`, `to` (`~/` is the home folder). Never overwritten. |
| `quit_on_output` | optional | For an emulator that goes back to a menu of its own when a game ends (the stock NNDDSS): a piece of a line of its output that says so. DSlot watches its output for it and asks it to quit (SIGTERM). |
| `[[options]]` | optional | Options DSlot shows as settings (below). |
| `[[actions]]` | optional | Actions on a game's files (below). |

A command is a list of words, the program first. These placeholders are filled in:

| | |
|---|---|
| `{rom}` | The ROM file, as found (a `.zip` too). |
| `{name}` | The ROM's file name without its extension. |
| `{system}` | The system's folder name: `NDS`, `GBA`, ... |
| `{saves}` | The game's saves folder, `<saves>/<system>/<ROM name>/`, made by DSlot before the game starts (nibOS: `/mnt/roms/Saves/...`, on whichever card the game is). It is the game's, shared by every emulator for its system (a game may move from one to another): keep the files other emulators put there, write your own resume state under your own name rather than over theirs, and never write battery RAM before reading what is there. Not on the stock OS, where the emulators keep their saves as they always have. |
| `{options}` | The words of the emulator's options as set for the game (each declared option set at some level, then the extra words, `args`, set at each level, the system's first); nothing if none is set. Only as a word of its own. |

An emulator is **installed** for a system when the program its command for that system starts
is there and runnable (a path, or a name found on `PATH`), and so is every file in
`requires`. DSlot reads the descriptions when it starts, and logs those it can't use.

The stock build ships descriptions for what the stock OS has: NNDDSS (`nnddss`, DS, priority
20: the default, as DSlot always did), DraStic (`drastic`, DS, 10), and RetroArch through the
stock menu's own launcher with mGBA (`mgba`, GBA, 20), gpSP (`gpsp`, GBA, 10) and Gambatte
(`gambatte`, GB and GBC, 20), and inkdot from its own stock zip (`inkdot`, GB and GBC, 30, run
through its `Roms/APPS/inkdot.sh` on the card the OS starts from). See [packaging/stock-card](../packaging/stock-card/Roms/APPS/dslot/emulators).

On nibOS each emulator's package ships its own: DSperate (DS, GBA) and inkdot (GB, GBC). Until
inkdot is packaged, [packaging/nibos/standin/gba-standin.toml](../packaging/nibos/standin/gba-standin.toml)
gives the GB and GBC pages a hand-built stand-in for testing (copied to `/etc/dslot/emulators/`
on the device, and deleted there once inkdot is installed).

SNES uses the stock folder name **`SFC`**. The stock build ships `snes9x.toml`, using the same
RetroArch launcher as mGBA. On nibOS the core/frontend package supplies its description;
[examples/nibcase-snes9x.toml](examples/nibcase-snes9x.toml) is the tested launch contract for
nibcase frontend. nibcase’s package hook generates the installed descriptor from its core’s
actual options. DSlot checks the available registry every two seconds while the shelf is up
and refreshes pages and settings when it changes; changes during a game are picked up on
return. Package installation/removal needs no manual DSlot configuration. See [snes.md](snes.md).

NES uses **`FC`**. nibcase’s `nibcase-nestopia` package registers it automatically, with
core options generated from the installed library. See [nes.md](nes.md).

## Only some of a system's games

An emulator that plays only some of a system's games lists them in a file of its own, named
under `[games]` by the system:

```toml
[games]
GW = "/usr/share/inktick/units.tsv"
```

One line a game, tab-separated: its ROM's name without the extension (a MAME set's name,
`gnw_dkjr`), its title, model, year and series; the last three may be empty, `#` lines are
comments. It then plays only those: it is their default (by its priority, as usual), the
others go to the next emulator that takes them, and a game's Game page offers only the
emulators that take it. The titles name the games on the shelf, the model, year and series
under them. A list that can't be read lists nothing.

## Leaving a game, and being put away

An emulator returns to DSlot by exiting; with DSperate, Menu saves the game's auto-resume state
and quits, and the next start resumes from it. **On SIGTERM an emulator should do the same:
save the auto-resume state and exit.** That is how DSlot puts a game away for the power menu
(on nibOS, a long press of power): it sends SIGTERM, waits up to 10 s, then kills it; Cancel
in the power menu, or waking from Sleep, starts the game again and it resumes. systemd sends
SIGTERM at shutdown too.

## Resume points

The top screen shows a game's resume point as a picture inset over its box, with when it was
saved (`src/resume.rs`). DSlot reads the resume point the next start will use: the chosen
emulator's, in the game's saves folder (`{saves}`). It reads only the header and the picture,
never the state itself, on a thread of its own, and keeps the result until the menu closes (a
resume point may have been cleared there). DSlot knows three formats, by the emulator's command
(`dsperate`, `inkdot`, `nibcase`). A resume point with no picture isn't shown.

| Emulator | File | Picture | Saved |
|---|---|---|---|
| DSperate | `<game code>.auto.dss` (the newest; the game code, not the ROM's name) | `<game code>.auto.thumb` beside it: `DSTHUMB1`, width and height (u32 LE), RGBA. A DS game's is both screens, 256 × 384; a GBA game's 240 × 160. An older file without the header is 256 × 192 RGBA, the top screen. Ignored if older than the state. | the state file's time |
| inkdot | `<ROM name>.auto.state` | `IKSL` 1, made (u64 LE, Unix seconds), play time (u64 LE), the PNG's length (u32 LE), the PNG (160 × 144) | made |
| inktick | `<ROM name>.auto.state` | as inkdot's, under `ITKS` 1: the PNG each screen 256 × 192, a two-screen unit's stacked | made |
| nibcase | `<ROM name>.auto.state` | `NIBSTATE`, version 1 (u32 LE), the lengths of metadata, picture and state (u32 LE each), metadata (TOML), the PNG (the core's frame, scaled into 256 × 240) | metadata `created` |

nibcase keeps a resume point per core. `<ROM name>.auto.state` is the one made by the last core
to run the game (its metadata's `core` says which), and the others are set aside as
`<ROM name>.auto-<core>.state`. DSlot shows the chosen core's own: the current one if that core
made it, else the core's file set aside. An emulator with a resume point of its own can be read
the same way by adding its format to `resume.rs`.

## Options

Each `[[options]]` entry becomes a setting on the console's page and the Game page of DSlot's
menu, with its key `<emulator>.<key>` in settings.toml (`[console.NDS.dsperate] hires3d = 2`).
Unset at every level, the option is not passed at all, and the emulator does what it does by
itself: the menu shows *DSperate's own* (the emulator's name). Set, it is passed every time
the game starts, and wins over whatever the emulator's own menu says for that game.
`default`, if given, says what the emulator does when not told; the first step of an unset
row lands on it.

| Key | | |
|---|---|---|
| `key` | required | Its name in settings.toml: letters, digits, `-` and `_`. |
| `label` | required | Its row's label in the menu. |
| `systems` | default: all the emulator's | The consoles it is for. |
| `kind` | `choice` (when `values` is given), `toggle` or `number` | |
| `values`, `labels` | a choice's | Its values (numbers or strings) and, optionally, how each is shown. |
| `min`, `max`, `step`, `unit` | a number's | Whole numbers from `min` to `max` (step 1 unless given), shown with `unit` after them. |
| `default` | optional | What the emulator does when not told: where an unset row's first step lands. Never passed. |
| `args` | required but for a toggle that is all off | The words it adds to the command, `{value}` standing for its value; a toggle's words when on. |
| `off` | a toggle's | Its words when off. |

### One place for each option: DSlot's settings

A declared option has one home, DSlot's settings.toml; an emulator keeps in its own files only
what it doesn't declare. An emulator with a menu of its own that changes a declared option
writes it there too, so the two menus can't disagree:

- Find the file with `nibkit::dslot::settings_file()` (`~/.config/dslot/settings.toml` on nibOS)
  and change it with `nibkit::settings::Settings`: load, `set`, `save` (which replaces the file
  whole, so it is never seen half written).
- Write under the emulator's id, at the console's level or the game's, the level its menu says
  it sets: `[console.GB.inkdot]`, `[game."GB/Tetris DX (World)".inkdot]` (the game's ROM name,
  without `.zip` or the `.gbc` under it). Never at `[system]`: DSlot doesn't read options there.
  Only values the option declares.
- "Reset" in the emulator's menu removes the key at that level (`Settings::reset`); with it
  gone at every level the option is unset again: not passed, the emulator's own (its `default`).
- Load the file when the game starts, write it when an option changes. DSlot doesn't write it
  while a game runs, and reads it again when the game exits.

So an option is set or it isn't, in one file: set, DSlot passes it on the command line (the
same value the emulator would read from the file), and both menus show it; unset, nothing is
passed and both show the emulator's own.

## Actions

Each `[[actions]]` entry is a row on the Game page: A asks, A again does it. `delete` lists
files to delete, with the command's placeholders; `*` stands for any run of characters in the
last part of a path. An action that names `{saves}` is only shown where DSlot has a saves
folder (nibOS). `systems` limits it to some consoles. DSlot also has an action of its own
there, **Delete saves**, which deletes the game's whole saves folder.

## Choosing

For a game, DSlot uses the first of these that is installed for its console:

1. the emulator set for the game (the Game page's Emulator row);
2. the one set for its console (the console's page);
3. the console's default: the installed one with the highest priority.

The Emulator row is there only when more than one emulator is installed for the console.

## launch.conf

DSlot 0.3's first builds, and DSperate's launcher before them, read options from
`~/.config/dslot/launch.conf` (`NDS = --hires3d 2`, `<game> = ...`, `<game> : <emulator>`).
The first time DSlot finds no settings.toml, it reads that file into it: options an emulator
declares become its settings (`--hires3d 2` is DSperate's 3D resolution, 2), any other words
its extra `args` at the same level, an emulator's own line goes to every console it plays, and
`<game> : <emulator>` is the game's emulator. The file is then renamed `launch.conf.old`.

Mega Drive (`MD`), Game Gear (`GG`) and Atari Lynx (`LYNX`) follow the same contract.
The nibcase package hook generates BlastEm, Gearsystem and Handy integrations; installing a
core adds its page and options, and removing it hides that page when no other emulator remains.
See [Sega and Lynx support](sega-lynx.md) for formats, firmware and validation limits.
Game & Watch (`GW`) too, through `nibcase-gw`: [Game & Watch support](game-watch.md).
