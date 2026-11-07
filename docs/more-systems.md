# More systems

| System / tab | Folder and save ID | Files | nibOS integration |
|---|---|---|---|
| Famicom Disk System / FDS | `FDS` | `.fds`, `.zip` | `nibcase-nestopia` (needs `disksys.rom` in `BIOS/`) |
| Master System / SMS | `SMS` | `.sms`, `.zip` | `nibcase-gearsystem` |
| SG-1000 / SG | `SG1000` (the cards have no such folder yet) | `.sg`, `.zip` | `nibcase-gearsystem` |
| PC Engine / PCE | `PCE` | `.pce`, `.zip` | `nibcase-beetle-pce-fast` |
| PC Engine CD / PCE CD | `PCECD` | `.cue`, `.chd` (never zipped) | `nibcase-beetle-pce-fast` (needs `syscard3.pce` in `BIOS/`) |
| PlayStation / PS | `PS` | `.m3u`, `.chd`, `.cue`, `.pbp` (never zipped) | `nibcase-pcsx-rearmed` (a BIOS such as `scph5501.bin` in `BIOS/` is optional) |
| PlayStation Portable / PSP | `PSP` | `.iso`, `.cso`, `.chd`, `.pbp` (never zipped); a download as `<Name>/EBOOT.PBP` | nibcase with the PPSSPP core (no BIOS) |
| Neo Geo Pocket / NGP | `NGP` | `.ngp`, `.ngc`, `.zip` | `nibcase-beetle-ngp` |
| WonderSwan / WS | `WS` | `.ws`, `.wsc`, `.zip` | `nibcase-beetle-wswan` |

Put games in `Roms/<folder>` on either card, and BIOS files in `BIOS/` at the top of either card.
Names come from the files, less what is in brackets; box art goes in `Imgs/` as for every
system. A CD game is its cue sheet (the `.bin` tracks it lists are not shown as games of their
own), a CHD, or a playlist (`.m3u`) of a game's discs, whose discs are not shown as games of
their own either.

A PSP game is its UMD's image (`.iso`, `.cso` or `.chd`) or an `.pbp`. A game from the
PlayStation Store is usually a folder of its own with an `EBOOT.PBP` in it:
`Roms/PSP/<Name>/EBOOT.PBP`. DSlot lists it as one game, named after the folder, not "EBOOT".
Its box art is `Roms/PSP/Imgs/<Name>.png`, beside the other PSP games', and its saves folder is
`Saves/PSP/<Name>/`. Only that one level is looked in: a folder in a folder is not. A folder
with no `EBOOT.PBP` in it is passed over. A folder copied in while DSlot runs shows up when its
`EBOOT.PBP` is in place by the time DSlot looks; if it doesn't, **Rescan games** finds it.

The PlayStation's and PC Engine CD's games are drawn as discs on the shelf, their print the box
art (`Imgs/`) or a label (`Labels/`). The PSP's are drawn as UMDs, to a UMD's proportions: the
60 mm disc in its 64 x 65 mm shell, half round at the top and square at the bottom, the same print
seen through the milky shell, a white "UMD" cap on the hub, the PSP's mark at the bottom left,
the shell's notch at the bottom right and its yellow arrow at the top left.

DSlot draws no carts for the other systems yet: a disk is no cart, and the Master
System's, PC Engine's, Neo Geo Pocket's and WonderSwan's carts have no shells drawn. Their pages
show boxes, so the Carts view shows the cover grid instead; the lists work as everywhere else.
A system's page appears once an emulator describing it is installed and it has games on a card.
