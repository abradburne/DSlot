# Artwork

Artwork is optional. With none, every cart gets a sticker printed from the ROM's own title, 32×32 icon and game code, and every box is drawn from the same icon.

All artwork sits beside the ROMs and is named after the ROM file, without its extension:

```
Roms/NDS/Example Game (USA).nds
Roms/NDS/Imgs/Example Game (USA).png      box front
Roms/NDS/Labels/Example Game (USA).png    cart label
Roms/NDS/Carts/Example Game (USA).png     finished cart (optional)
```

`Imgs` is the folder the stock menu already uses for box art, so one set of covers serves both.
Every system's artwork is laid out the same way beside its ROMs (`Roms/GBA/Imgs/...`).
Every DS box is drawn the shape of a real DS case (137 : 125); a cover of another shape is scaled to fill it and cropped about its centre.

## Cart label format

A label is the sticker alone, as printed: logo band, artwork and the strip with the game code. The frontend supplies the plastic.

| | |
|---|---|
| File | `Labels/<ROM filename without extension>.png` |
| Format | PNG, 8-bit RGBA, sRGB |
| Content | The whole sticker, edge to edge, upright. Nothing of the cart shell. |
| Transparency | Transparent outside the sticker's outline (its rounded corners). |
| Shape, DS | 272 : 296 (width : height) |
| Size | 272 × 296 or larger at the same shape. 544 × 592 is plenty for a 1024 × 768 screen. |

A frontend stretches the label to its shell's sticker window, so a label a few percent off the nominal shape still fits.

`Carts/<name>.png` is the alternative for art that already includes the shell: a complete cart front, 336 : 360, transparent outside the cart. When present it is drawn as it is and `Labels` is ignored.

These shapes are the ones GameTDB uses for its DS `label` and `cart` images, which are keyed by the four-letter game code in the ROM header. A tool can therefore fetch the right label for a ROM without matching names.

## Cart Studio compatibility

DSlot reads what slot CFW's Cart Studio (<https://studio.slot-cfw.fyi>) makes, so labels and
shells made with it work here as they do in slot. The format is slot's, as its card's
`Labels/README.txt` gives it; credit for it goes to slot.

**Labels.** Cart Studio's labels sit at the card's root, a folder per system, named after the
ROM: `Labels/GBA/Metroid Fusion.png`, `Labels/GB/...`, `Labels/GBC/...`. Its sizes are
1280 × 661 for a GBA label (1.94 : 1) and 1024 × 901 for a Game Boy or Game Boy Color one
(about 1.13 : 1); a label of any other size is scaled to fill the sticker and cropped from the
centre, as slot does. A game's label is looked for in this order:

1. DSlot's own, beside the ROM: `Roms/GBA/Labels/<rom>.png` (stretched to the sticker, above).
2. Cart Studio's, on the game's own card: `<card>/Labels/GBA/<rom>.png`.
3. Cart Studio's, on the other card.

**Shells.** `cart_shell.txt` gives a game's cart a shape, colour and finish, one line a game:

```
Metroid Fusion = auto 6a3fa0 solid
Tetris = notched 7b3fb0 clear
Pokemon - Crystal Version (USA) = rounded 1fa39a glitter
```

| Field | Values | |
|---|---|---|
| Name | the ROM's name without its extension | |
| Outline | `auto`, `notched`, `rounded` | the Game Boy cart's shape, with the notch cut from its corner, or the Color-only cart's, rounded all round. `auto` is the system's own: notched on the GB page, rounded on the GBC page. The GBA cart has one shape. |
| Colour | hex RGB, `6a3fa0` (or `#6a3fa0`) | the plastic's colour. Anything else keeps the system's own. |
| Finish | `solid`, `clear`, `glitter` | opaque; see-through, the board, its chips and the gold contacts showing faintly in the plastic's colour, the shelf showing where there's no board, and the edges catching the light; or flecked with metal flake, with sparkles. |

A line that doesn't read is skipped, and a word that doesn't is taken as `auto` or `solid`;
lines starting `#` are comments. The first file with a line for the game wins, looked for:

1. beside the ROMs: `Roms/GBA/cart_shell.txt`;
2. then on the game's card, then the other: `Labels/GBA/cart_shell.txt`, `Labels/cart_shell.txt`
   (beside Cart Studio's labels, where slot's README describes it), and the card's root.

The colour and finish are laid over the system's own shell, keeping its relief (ribs, well,
triangle, lit and shaded edges), so they work for every system DSlot draws carts for: a
`cart_shell.txt` line for a Game Gear, Lynx or SNES game colours its cart the same way. The
outline only chooses between the Game Boy's two shapes. A shell is drawn once per system,
colour and finish and kept while DSlot runs. The glitter is still, not animated, so a resting
shelf draws nothing.

## The shell

The cart's plastic is drawn by DSlot itself, in `src/art.rs`, to the proportions of a real card, on a 336 × 360 grid with the sticker's window at (32, 20), 272 × 296. No artwork of any game, and no image of the card, ships with DSlot.

Every cart shell (DS, GBA, Game Boy, SNES in both shapes, NES, Mega Drive, Game Gear, Lynx) is drawn the same way: as shapes in code, to proportions measured from photographs of real carts. The photographs were only references. None of them, and no image derived from them, is in the repository or the package. What each shape was measured from is noted beside its code and in the system's own page ([snes.md](snes.md), [nes.md](nes.md), [sega-lynx.md](sega-lynx.md)). The only photographs DSlot ships are the system cards' console photos: freely licensed, and credited ([system-cards.md](system-cards.md)).

## GBA

A GBA box is drawn square and in card, not in a case; box art of another shape is scaled to
fill it and cropped about its centre. `Labels/` and `Carts/` work as for DS; the shapes are
1314 : 688 for a label and 1820 : 1052 for a finished cart. With no label of its own, the
sticker is printed from the box art: the cover on the left over a blurred, dimmed copy of
itself, name, maker and game code beside it.

The GBA cart is drawn by DSlot too (`src/art.rs`, `gba_frame`), measured from a photo of a real
one (AGB-B8ME in Gekkio's Game Boy hardware database): charcoal plastic, wider and shorter than
the DS card; the grip along the top the full width with large rounded corners, the body stepping
in a little below it; the grip's lighter face with its crease turning up toward the corners;
the label's well; the triangle pressed in below it, pointing into the slot. The console's name,
embossed under the grip on the real cart, is left out.

## Game Boy and Game Boy Color

Boxes, labels and carts work as for GBA; a label is 442 : 372 (GB) or 422 : 380 (GBC), a
finished cart 570 : 650. With no label of its own, the box art fills the sticker: the two are
about the same shape. Box art goes by the ROM's name without `.zip`, and without the `.gb`
or `.gbc` under it (`Tetris DX (World).gbc.zip` looks for `Imgs/Tetris DX (World).png`).

Both carts are drawn by DSlot (`gb_frame`, `gbc_frame`), measured from photos in the same
database. The Game Boy cart (DMG-TRA-1, Tetris): warm grey, 57 × 65 mm, a notch cut from its top
right corner, five ribs at each side along the top for the grip, a plate pressed in between
them (its embossed name left out), the label's well, and the triangle pressed in below it. The
black and coloured carts of later games are the same mould, drawn grey. The Color-only cart
(CGB-AHYE-0) is clear, on a mould of its own: no notch, plate or grip on its face, the label
lower down, and the board showing through the smoky plastic, its chips above the label and its
contacts below.
