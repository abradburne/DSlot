# Game & Watch

| System / tab | Folder and save ID | Files | nibOS integration |
|---|---|---|---|
| Game & Watch / G&W | `GW` | `.mgw`, or a `.zip` holding one; MAME sets (`gnw_dkjr.zip`, `gnw_dkjr.7z`) | `inktick` (the SM510-family units it lists), else `nibcase-gw` (gw-libretro) |

Put the units in `Roms/GW` on either card. An `.mgw` file has no header to read, so a unit is
named after its file, less what is in brackets: `Donkey Kong (Nintendo, Multi Screen).mgw` is
**Donkey Kong**. A MAME set is named by the emulator that lists it: inktick's list
(`/usr/share/inktick/units.tsv`) gives each set's title, model, year and series, so
`gnw_dkjr.7z` is **Donkey Kong Jr.**, with "DJ-101 · 1982 · New Wide Screen" under it. Box art goes in `Imgs/` beside them as for every other system, named after the set
(`Imgs/gnw_dkjr.png`); without any, a box is printed with the unit's name.

Game & Watch art is usually a photo of the unit rather than of its box, and the units come in
every shape: a Panorama Screen stands tall, a Multi Screen opens out wide. So a picture is shown
whole, fitted inside the landscape box over a blurred, dimmed copy of itself, rather than
cropped to fill it as other systems' covers are. libretro-thumbnails' `Handheld_Electronic_Game`
set has such photos named after the unit (`Donkey Kong Jr. (Nintendo, Panorama Screen).png`):
copy one to its set's name.

A Game & Watch unit is the game itself, with no cart to draw, so its page shows boxes alone,
as the Apps page does: the **Carts** view shows the cover grid instead, and the two list views
work as everywhere else. The boxes are landscape, as a unit is. A plays the unit.

Two emulators share the page. inktick plays the units it lists (the 41 on Sharp's SM510, SM511
and SM512): it is their default, at a higher priority. Everything else (the SM5A units, every
`.mgw`) goes to nibcase-gw, which is also all a listed unit can be switched to on its Game page.
How an emulator lists the games it plays: [emulators.md](emulators.md) (`[games]`).

nibcase plays its units, and splits the two-screen ones (Donkey Kong, Mario Bros., Lifeboat,
Donkey Kong II and the like) across both panels. Installing `nibcase-gw` registers its command
and options with DSlot (`nibcase-gw.toml`), which adds the page; settings and saves use `GW`.
