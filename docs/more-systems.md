# More systems

| System / tab | Folder and save ID | Files | nibOS integration |
|---|---|---|---|
| Famicom Disk System / FDS | `FDS` | `.fds`, `.zip` | `nibcase-nestopia` (needs `disksys.rom` in `BIOS/`) |
| Master System / SMS | `SMS` | `.sms`, `.zip` | `nibcase-gearsystem` |
| SG-1000 / SG | `SG1000` (the cards have no such folder yet) | `.sg`, `.zip` | `nibcase-gearsystem` |
| PC Engine / PCE | `PCE` | `.pce`, `.zip` | `nibcase-beetle-pce-fast` |
| PC Engine CD / PCE CD | `PCECD` | `.cue`, `.chd` (never zipped) | `nibcase-beetle-pce-fast` (needs `syscard3.pce` in `BIOS/`) |
| PlayStation / PS | `PS` | `.m3u`, `.chd`, `.cue`, `.pbp` (never zipped) | `nibcase-pcsx-rearmed` (a BIOS such as `scph5501.bin` in `BIOS/` is optional) |
| Neo Geo Pocket / NGP | `NGP` | `.ngp`, `.ngc`, `.zip` | `nibcase-beetle-ngp` |
| WonderSwan / WS | `WS` | `.ws`, `.wsc`, `.zip` | `nibcase-beetle-wswan` |

Put games in `Roms/<folder>` on either card, and BIOS files in `BIOS/` at the top of either card.
Names come from the files, less what is in brackets; box art goes in `Imgs/` as for every
system. A CD game is its cue sheet (the `.bin` tracks it lists are not shown as games of their
own), a CHD, or a playlist (`.m3u`) of a game's discs, whose discs are not shown as games of
their own either.

DSlot draws no carts for these systems yet: a disk or a disc is no cart, and the Master
System's, PC Engine's, Neo Geo Pocket's and WonderSwan's carts have no shells drawn. Their pages
show boxes, so the Carts view shows the cover grid instead; the lists work as everywhere else.
A system's page appears once an emulator describing it is installed and it has games on a card.
