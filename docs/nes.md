# NES / Famicom

The **NES** page uses `Roms/FC` on either card, matching the stock system id and nibcase's
`--system FC`. It accepts `.nes` (iNES and NES 2.0), `.unf`, `.unif` and single-cartridge `.zip`
files, case-insensitively. `Star Lantern.nes.zip` uses `Star Lantern` for artwork, settings and
saves. Disk System `.fds` files and NSF music are outside this cartridge integration.

NES ROM dumps have no universal title, publisher or printed serial fields. DSlot displays the
tidied filename and does not invent header metadata. Invalid files remain visible by name so
a core can report the loading error, as with other consoles.

The shelf draws a grey front-loading cartridge with a tall label and side grip. Boxes are
portrait. `Imgs`, `Labels` and `Carts` overrides work as on the other pages; grid covers keep
their proportions. `--make-demo` includes fabricated NES headers for shelf previews, not games.

On nibOS install `nibcase` and `nibcase-nestopia`. The package hook queries the installed core
and generates `/usr/share/dslot/emulators/nibcase-nestopia.toml`, including its option rows.
DSlot's existing registry refresh notices installation/removal and rebuilds the page and
settings automatically. The minimal [example](examples/nibcase-nestopia.toml) shows the CLI.
The stock build has a Nestopia/RetroArch descriptor, enabled only when that launcher and core
are actually present.

Settings use `[console.FC]`, `[game."FC/<ROM stem>"]` and the emulator's `nibcase-nestopia`
option namespace. Saves use the game's shared `Saves/FC/<ROM stem>/` folder. The same battery,
resume, pause-menu and SIGTERM handling used for SNES applies to NES.

Nestopia's default composite filter is disabled by nibcase for raw-pixel integer scaling; it
remains selectable in core settings. Core binaries and games are not bundled with DSlot.
See the sibling nibcase validation record for measured tests and hardware limitations.
