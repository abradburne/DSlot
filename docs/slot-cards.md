# Using a card set up for slot

slot CFW (<https://slot-cfw.fyi>) is a Game Boy frontend for Anbernic's other handhelds. A card
set up for it works in DSlot as it is, next to the stock OS's layout: both can be on the same
card, or one on each. Nothing on the card is moved or changed.

| On a slot card | In DSlot |
|---|---|
| `Games/GBA/`, `Games/GB/`, `Games/GBC/` | read as ROM folders, as `Roms/GBA/` and the rest are |
| `Labels/<system>/<rom>.png` (Cart Studio's labels) | the carts' labels, after DSlot's own ([artwork.md](artwork.md), Cart Studio compatibility) |
| `Labels/cart_shell.txt` | the carts' outline, colour and finish (the same section) |
| `BIOS/` | the same folder DSlot's emulators look in already |
| battery saves | not yet: see below |
| save states | not carried over: see below |

**Games.** Each card's `Games/` is looked in after both cards' `Roms/`, on nibOS and on the
stock OS, by DSlot and by the emulators it starts: the list comes from nibkit
(`nibkit::paths::rom_roots`). A game whose file is in `Games/<system>/` and also in a
`Roms/<system>/` is listed once, as the one in `Roms/`. slot reads unzipped ROMs only; DSlot
also takes zipped ones in either folder.

A game's settings, play time and rating are kept by its system and ROM name, so they are the
same whichever folder it's in. Its saves go where every game's do: `Saves/<system>/<ROM name>/` in the
one saves folder (on nibOS the ROMS partition's `Saves/`), whichever folder or card the game
is in.

**Battery saves.** slot's guide doesn't say where it keeps a game's `.sav`, so DSlot's
emulators don't pick slot's saves up yet. Until they do, copy a game's `.sav` into its
`Saves/<system>/<ROM name>/` yourself, named after the ROM (`<ROM name>.sav`), to carry its progress across.

**Save states** are not portable. A state is a snapshot of one emulator's memory, in that
emulator's own format: slot's (mGBA's and gpSP's) can't be loaded by inkdot or DSperate, nor
theirs by slot. Battery saves are the game's own, and do carry over.
