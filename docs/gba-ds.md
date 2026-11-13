# GBA DS

[GBA DS](https://github.com/kyannon/gba-ds) (GPL-3.0) is a Game Boy Advance emulator for
two-screen handhelds: the game plays on one screen, and its own map, race HUD or status screen
stays up on the other. DSlot gives it a page of its own, **GBA DS**, after the GBA's.

- **Its games** are the ones in `Roms/GBA`, on either card, that GBA DS has a second screen
  for: it knows a game by the four-character code in its cart's header, and DSlot keeps the
  codes of its supported games (`src/rom/gbads.rs`, from its
  [list](https://github.com/kyannon/gba-ds/wiki/Supported-games) as of GBA DS 1.5: 51 games,
  their regional releases, and romhacks that keep the code). Nothing is copied or moved: the
  same files are on the GBA page too, with the same box art, labels and carts. A folder of games
  is on the page when it holds one of them.
- **A game it adds later** is on the page once its code is added to that list; until then it
  is on the GBA page alone.
- **The page is there** when an emulator description for the `GBADS` system is installed
  (docs/emulators.md) and a card has such a game. DSlot makes no `Roms/GBADS` folder.
- **Settings, emulator options and the saves folder** go by `GBADS`, not `GBA`
  (`[console.GBADS]`, `[game."GBADS/Metroid Fusion (USA)"]`, `Saves/GBADS/<ROM name>/`), so
  what is set for a game on one page doesn't change it on the other. Play time, the rating and
  favourites go by the file, so they are the same on both.
- **RetroAchievements** aren't looked up on this page; GBA DS has a sign-in of its own.
- **Card:** a neutral one (`data/systems/gbads.toml`), with no stats or photo, since it isn't a
  console.

## Starting games

DSlot starts a game through the description's command for `GBADS`, as for any system. On
nibOS, GBA DS's package (nibOS's `rootfs/gba-ds`) installs the program in `/usr/lib/gba-ds`
and this description, shortened:

```toml
name = "GBA DS"
priority = 100
requires = ["/usr/lib/gba-ds/gba-ds"]

[systems]
GBADS = ["/usr/lib/gba-ds/run-game", "{options}", "{rom}", "{saves}"]
```

`run-game` starts GBA DS itself rather than through its own `launch.sh`, which is written for
ROCKNIX, BananaOS and the stock OS. It gives GBA DS a window on each panel through SDL's
KMSDRM driver, with SDL's atomic KMS path off (`SDL_KMSDRM_ATOMIC=0`, as DSlot and DSperate
have it; with it on, GBA DS 1.5 crashed creating its second window). It also reads the
touchscreen straight from its device, keeps the battery save and resume point in the game's
saves folder, and puts GBA DS's other files in `~/.local/share/gba-ds`. GBA DS takes a `.zip`
as it is, and on SIGTERM saves its resume point and exits.

The stock build ships no description for it. Off the device (trying the shelf on a PC) every
page shows without one.
