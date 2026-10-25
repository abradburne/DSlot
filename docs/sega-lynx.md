# Mega Drive, Game Gear and Atari Lynx

| System / tab | Folder and save ID | Cartridge formats | nibOS integration |
|---|---|---|---|
| Mega Drive / MD | `MD` | `.md`, `.gen`, `.bin`, `.smd`, `.zip` | `nibcase-blastem` |
| Game Gear / GG | `GG` | `.gg`, `.zip` | `nibcase-gearsystem` |
| Atari Lynx / Lynx | `LYNX` | `.lnx`, `.zip` | `nibcase-handy` |

Folders live under `Roms/` on either card. A ZIP holds one cartridge. Filename-based names,
box artwork in `Imgs/`, ratings, favourites, cache files and per-game settings follow the
existing shelves. Each system has its own cartridge shell, drawn after a real one, measured
from a photo: the Mega Drive's black and wide, its raised middle up to the top edge, a recessed
label with a grid band under the art; the Game Gear's charcoal, a raised band with two ridges
across the top and a blank oval badge under a label with a coloured strip down its left edge;
the Lynx's nearly square and dark grey-green, the label framed in black, gold contacts along
the bottom. No logos or trademarks are drawn.
No game code or firmware is distributed; demo shelves contain named placeholders only.

nibcase queries each installed core and generates its command and current option definitions
for DSlot. The running shelf discovers that registration automatically; no manual launch.conf
entry is needed. Game saves use `Saves/<ID>/<ROM stem>/`, including across core changes.

Handy needs the user's **512-byte `lynxboot.img`** in `BIOS/` at the root of either card.
nibcase searches both cards, reports missing firmware clearly, and supports an explicit
`--bios DIR`. Libretro rotation keeps portrait games and their state previews upright.
Handy's options include rotation, 60 Hz video updates, colour depth and ghosting.

The stock build includes descriptions for installed Genesis Plus GX, Gearsystem and Handy
cores through the stock launcher. Their required library paths keep absent cores hidden.
Stock launches have parser/command checks, not a physical stock-OS play test.

Tests cover scanning, zipped names, cache reloads, save/launch paths, installed descriptions,
and all cartridge-shell windows and proportions. nibcase's validation document records the
separate core measurements and firmware-dependent Lynx checks.
