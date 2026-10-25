# Apps

DSlot has an **Apps** tab when there is at least one app in `Roms/APPS` on either card (nibOS:
`/mnt/roms/Roms/APPS` and `/mnt/ds/Roms/APPS`; the stock OS: `/mnt/mmc/Roms/APPS` and
`/mnt/sdcard/Roms/APPS`). Apps from both cards are listed together, by name, as a grid of their
icons; favourites, ratings, play time and sorting work as for games.

On nibOS, a package can install an app in `/usr/share/dslot/apps/<app>/` (it can't write to the
cards), laid out the same way; those are listed with the cards' (PortMaster's is one).

An app is a folder in `Roms/APPS` with an `app.toml`:

```toml
name = "Tide Clock"
description = "A clock for the tides"   # optional, shown under the name
command = ["./tide-clock", "--big"]     # the first word relative to the folder, absolute, or on PATH
icon = "icon.png"                       # optional; icon.png if left out
runs_on = ["nibos"]                     # "nibos", "stock"; nibOS only if left out
[env]                                   # optional
```

An app that fetches itself the first time it runs (PortMaster) can go by another name until it
has: while `file` isn't there (`~/` is the home folder), DSlot shows `name`, and `description`
if given. When such an app exits, DSlot looks at everything again, so its own name shows at
once.

```toml
[until]
file = "~/.local/share/PortMaster/version"
name = "Get PortMaster"
description = "Downloads it over Wi-Fi"   # optional
```

```
Roms/APPS/Tide Clock/app.toml
Roms/APPS/Tide Clock/icon.png       square, drawn as the app's box
Roms/APPS/Tide Clock/tide-clock     the program
```

On the stock OS, the stock menu's own app scripts there (`Roms/APPS/<name>.sh`, with
`Roms/APPS/Imgs/<name>.png` as the icon) are apps too, run with bash; DSlot's own scripts are
left out. On nibOS those scripts are left out (they are the stock OS's), and so is any app
whose `runs_on` doesn't name nibOS.

DSlot starts an app as it starts an emulator (docs/emulators.md): it lets go of both screens
and the sound, runs the command in the app's folder (on nibOS inside dslot.service, with its
environment and limits), and takes them back when the app exits. On SIGTERM an app saves what
it must and exits: the power menu puts it away that way, and Cancel starts it again.

nibkit's `dslot` feature reads `app.toml` as DSlot does (`nibkit::dslot::App::parse`), so an
app can check its own in a test; nibOS's `sdk/run.sh` can install an app into `Roms/APPS`.
