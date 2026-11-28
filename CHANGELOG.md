# Changelog

## 1.0.0-beta1

The first beta of DSlot 1.0: a Home page across every console, artwork DSlot fetches itself,
games sent over Wi-Fi, grid sizes, discs that spin, and a much faster start.

- **Home**: a page at the left of the tabs (a house on its tab, the first blade in the system
  picker) with the games of every console together: Recent, Favourites, Rating and Name, picked
  by a tap on the bar under the tabs or with L2 and R2. Carts, discs and boxes of different
  consoles sit side by side in the Carts view, each its own shape; Grid, List and the top
  screen mark each game's console. On the Home blade the top screen shows a card of the whole
  library. Start opens Home's own view sheet (`[console.HOME]`). Apps and folders are not on it.
- **Name, Recent, Favourites, Rating on every console**: the same bar under the tabs on each
  console's page, in step with the console's show and sort settings. Arcade keeps its boards'
  sections where there is more than one board.
- **Artwork DSlot fetches itself** (Game, Console and System → Artwork): boxes, cart labels
  and discs for the games on the cards, matched by name, by the game code in a DS ROM, and by
  CRC-32 for DS, GBA, GB and GBC cart scans. From Libretro Thumbnails, GameTDB (DS covers and
  labels), art.slot-cfw.fyi (cart scans), and LaunchBox's games database for carts, discs and
  boxes the others lack, with No-Intro and FBNeo names to match by. It runs in the background:
  the shelf fills in as pictures arrive, a pill at the top right of the top screen shows the
  job, and it can be cancelled. An index of LaunchBox's pictures for the 23 consoles (25,953
  games, 2 MB) is installed with DSlot (`/usr/share/dslot/art/` on nibOS, `<app>/art/` on the
  stock OS; `tools/update-launchbox-art.py` refreshes it).
- **Receive games** (Network → Receive games, on nibOS): games sent from a phone or computer
  with LocalSend land in the right console's folder, through nibOS's `nibos-drop`. The page
  turns the receiver on and off and shows the device's name and address, an optional PIN,
  which card games go to, and the last transfers. With Fetch artwork on, a game that arrives
  gets its box and cart art by itself.
- **Grid sizes**: Small (6 × 4), Medium (5 × 3), Large (4 across) and Extra large (3 × 2), for
  everything or per console (Apps too), previewed as you choose from Start.
- **Discs spin**: PlayStation, Mega-CD and PC Engine CD discs spin up as a game starts and
  down when it comes back, with the drive's sound; a tap spins one. A PSP UMD's inner disc
  turns inside its shell.
- **Starting and resuming**: as a game goes in, the top screen shows its title over a dark
  scrim with "Starting…", "Resuming…" or "Starting fresh", held until the game draws.
  "Resuming…" only when there is a resume point on the card. Coming back from a game never
  flashes the old text.
- **Cart and box art drawn whole**: a finished scan in `Carts/` keeps its shape (disc
  consoles too, a PSP UMD for one); box art standing in for a cart label on GB, GBC, Game Gear
  and Lynx is shown whole over a blurred copy of itself where it would have been cropped; and
  Japanese GB, GBC, GBA and Super Famicom boxes are drawn in their own proportions. Grid
  thumbnails made before this are redrawn.
- **A faster start**: DSlot tells systemd when the first shelf frame is up (`Type=notify`),
  scans the games and loads its fonts while the screens and SDL come up, leaves what the first
  frame doesn't need until after it, and starts the Wi-Fi and Bluetooth watch a frame later.
  Thumbnails are cached LZ4-compressed (old ones are still read), and the system picker reads
  its blades once, not every frame.
- **Less memory, less power at rest**: a resting shelf no longer rescans the library or lays
  out text every frame (it looks for changes twice a second), the screensaver runs at 10
  frames a second, and the emulators' descriptions are re-read only when their folder
  changes. Thumbnail, resume-picture and cart caches are bounded, the CJK font loads when
  first needed, and memory is handed back before a game starts and after an artwork job.
- **Power off and restart on nibOS** go straight to systemd, so the goodbye picture is up in
  about a quarter of a second.
- `dslot.service` no longer waits for `systemd-remount-fs.service`.

## 0.8.0

- **GBA DS page**: a page after the GBA's for the games in `Roms/GBA` that
  [GBA DS](https://github.com/kyannon/gba-ds) gives a second screen (their map, HUD or status
  screen on the other screen), known by their cart codes: 51 games as of GBA DS 1.5. The same
  files as on the GBA page, with their art; its emulator, settings and saves go by `GBADS`.
  There once an emulator description for `GBADS` is installed (nibOS's `gba-ds` package has
  one). The shelf shows a GBA DS game's resume picture (the `.game.bmp` GBA DS writes beside its
  resume point). See [GBA DS](docs/gba-ds.md).

## 0.7.1

- Cargo.lock takes nibkit 0.5.0, which Cargo.toml names: 0.7.0's still pinned 0.4.1, so its
  package did not build with `--locked`.

## 0.7.0

- **System → Help**: What's new in the running nibOS and short help topics (the controls, the
  menu, saves, sleep and charging, Wi-Fi, adding games, the console), the last section under
  System. nibOS keeps them as text files (`/usr/share/nibos/whats-new.txt`, written from its
  changelog when an image is built, and `/usr/share/nibos/help/`); nibkit 0.5's `guide` reads
  them. A topic is read as a page of text with no highlight: up and down scroll it a line, left
  and right a screen, a finger drags it, and B goes back to the list. Where there are no files
  (the stock OS) there is no Help section.
- **Folders of games**: a folder in a console's folder (`Roms/GB/_chromatic`, and folders in
  it) is on the shelf as a folder, before the games, with how many games are in it. A opens it
  (or a tap on it, or a pull); B goes back out, onto the folder left, and the heading names the
  folder open. A folder with no games in it, a hidden one, DSlot's `Imgs`/`Labels`/`Carts`, and
  the Ports and Apps pages' folders are not shown. A game in a folder finds its art beside it,
  else in the console's `Imgs` and the rest; back from a game, the shelf is in its folder.
  Favourites alone and an arcade section list every game of theirs, in whatever folder.
- **`_hidden` folders**: a folder of that name, at any depth, is never on the shelf, nor
  anything in it.
- **Hide _ before folder names** (System → Shelf, on by default): `_chromatic` shows as
  `chromatic`, on the folder, the top screen and the heading. Its picture is still named after
  the folder itself (`Imgs/_chromatic.png`).
- **B never starts a game**: it used to insert the cart, as A does.
- **Start a game fresh**: hold Select and press A (or hold Select as the cart goes in) and the
  game starts from boot, not from its resume point; "Starting fresh" shows as the cart goes
  in. The resume point is set aside, not deleted: renamed `<file>.before-fresh` in the game's
  saves folder, one kept, for DSperate, inkdot, inktick and nibcase games. Select tapped alone
  still opens the system picker; let go after another button, it no longer does. In a desktop
  preview, S stands in for Select the same way.

## 0.6.0

- **Both screens lit when the shelf comes back**: a player turns off a screen it doesn't use
  and lights it again before it exits; one that is killed or crashes left the bottom screen
  dark. DSlot now powers both backlights on when a game or app returns, and when it starts (a
  player killed along with the DSlot that started it).
- **zip 9.4** (from 2.x), here and through nibkit 0.4.1's nib-cheevos, for the zip crate's
  advisories (zip cloaking, the AES HMAC check at an early end of file, unbounded symlink
  targets). Zipped games are only read into memory, never extracted to disk.

- **Mega-CD, 32X and Pico**: three pages for PicoDrive's other systems, played by nibcase with
  `nibcase-picodrive`. Mega-CD discs (`.chd`, `.cue`, `.iso`, `.m3u`) in `Roms/SEGACD`, drawn as
  discs; their folder's README names the BIOS one a region (`bios_CD_U.bin`, `_E` or `_J`). 32X
  carts (`.32x`) in `Roms/32X`, drawn as Mega Drive carts. Pico carts (Mega Drive dumps) in
  `Roms/PICO`, known by their folder: boxes alone. Box art from libretro's thumbnails, and a
  system card each (the Pico's photo by Muband, CC BY-SA 3.0). A `.cue` or `.md` outside these
  folders is still the PC Engine CD's or the Mega Drive's.
- **Arcade**: a page for FinalBurn Neo's sets, `Roms/ARCADE` (`.zip`, `.7z`), played by nibcase
  with `nibcase-fbneo`. Sets are named by the core's list: the title, and under it the maker,
  year and board. A board's BIOS set beside the games (`neogeo.zip`) isn't shown. Boxes alone.
- **Sections on the shelf**: the Arcade page has a row of sections under its tab, All and then
  each board its games are on (Neo Geo, CPS-1, CPS-2, ...; the most games first, Other last),
  stepped through with L2 / R2 or tapped, as the menu's sections are. The one chosen stays with
  the page; All until one is. The grid and the list move down under the row.
- **The volume and brightness indicator** (nibkit 0.4.0's `menu::level`): on the shelf, when
  the volume keys or Menu + volume change a level, nibkit's indicator shows at the bottom right
  of the top screen (a speaker or a sun, a bar, the percent) and fades 1.5 s after the last
  press.

- nibkit 0.3.1, with nib-cheevos' `chd` and `cso`: the shelf finds the sets of PSP games and of
  every CHD disc image (PlayStation, PC Engine CD, PSP), which had no hash before.

- **The PSP**: a page for the PlayStation Portable, `Roms/PSP`, played by nibcase with the
  PPSSPP core (its package writes the emulator description). A game is a UMD's image (`.iso`,
  `.cso`, `.chd`) or an `.pbp`, never zipped. A download in a folder of its own,
  `Roms/PSP/<Name>/EBOOT.PBP`, is one game named after the folder; its box art and saves go by
  that name, beside the other PSP games'. The games are drawn as UMDs: the 60 mm disc in its
  64 x 65 mm shell, half round at the top and a wide arc at the bottom, the print (label or box art) seen through the milky
  shell, a white "UMD" cap on the hub, the notch and the yellow arrow. Its box on the top
  screen is a UMD keep case's shape, 104 x 177 mm, tall and narrow. The PSP comes right after the PlayStation in the tabs and the system picker. Its
  card has the PSP-1000's photo (Evan Amos, public domain). Its RetroAchievements sets show on
  the shelf (nibkit 0.3.1 hashes PSP games).

## 0.5.0

- **Sections in the menu**: a page with more than a screen's worth is cut into sections, in a
  row under the tabs (nibkit 0.3.0's `Menu::sections`); L2 / R2 (the triggers, read as SDL's
  trigger axes; `[` and `]` on a keyboard) or a tap go through them, L / R still change tab.
  System's 25 rows are **Shelf**, **Screen**, **Sound**, **Power**, **Time** and **About**, and
  **Update** is System's last section rather than a tab; a console's page is **Emulator** and
  **Shelf**. Wi-Fi and Bluetooth are one tab, **Network**, with **Network info** as its third
  section. The pages that opened from a row (Date and time, System info, Network info) are
  sections now, so their rows are gone and B closes the menu from them as from any other.
  The Game page stays one part.
- **Flicked carts carry on**: on the shelf, a flick of the carts keeps them going after the
  finger lifts, slowing as a flicked grid or list does, to rest on a cart (held at either end),
  where before they stopped at the nearest one. The selection goes along with them; a tap while
  they move stops them. A flick no longer goes on past three seconds without a touch.
- **nibkit 0.2.2**: the menu's Achievements tab is wide enough for its label, with room either
  side; the other tabs stay as they were, and a tap finds the tab drawn there
  (`layout::tab_at`).
- **Earned achievements**, a row on the Achievements page while signed in: every achievement
  earned in the games the shelf has looked up, the latest first, with the game and its points,
  and the highlighted one's description on the title line. Read from what the lookups keep, so
  it needs no network.
- **RetroAchievements through nibkit's `nib-cheevos`** (nibkit 0.2.0), the same code the
  emulators use: a ROM's hash is rcheevos' rhash (`achievements/hash.rs` is gone), so the shelf
  now finds the sets of every console the service has, SNES, NES, Mega Drive, PC Engine,
  PlayStation (from a cue sheet) and the rest, as well as DS, GBA, GB and GBC; the sign-in, the
  game's sets (`achievementsets`, the request rcheevos 12.4 makes instead of `patch`, with any
  bonus set) and the player's unlocks are built and read by rcheevos (`rc_api_*`), not by hand;
  the token file is `nib-cheevos`' (`launch`), the same format. The User-Agent is
  `DSlot/<version> (nibOS; linux aarch64) rcheevos/12.4`.
- **The sign-in goes only to an emulator that earns achievements** for the game's system, as
  its description now says (`[achievements] systems = [...]`, docs/emulators.md); and
  `RA_HARDCORE=1` only to one that says `hardcore = true`. The Achievements page's *Earned in*
  note is made from the installed descriptions.

- **Back opens the menu.** The DS+'s Back key, beside Menu, opens the menu from the shelf and
  goes back in it as B does. Back is a key of
  its own, not a gamepad button: SDL3 calls it AC Back, and sdl2-compat passes it to SDL2
  programs as Sleep, so DSlot takes both.
- **Start opens the View sheet**: how the page is shown (View, Sort by, Show, Game case), on
  the top screen in the game's details' place, so the shelf on the touch screen changes as each
  row does. They are the console page's settings, as in the menu, with Y back to what they
  inherit; Start, B, Back or a touch closes it. Start no longer plays: A does.
- **X opens the game page**, and the box on the top screen turns round to its **back**: the
  title on a band in the system's colour with your rating and favourite heart (and a ribbon on
  your most played game), its title screen and a screenshot in play in white borders at an
  angle, a blurb, your own play as the points down the bottom, and a barcode. The year,
  developer, publisher, genre and blurb are from Wikipedia and Wikidata, the screenshots from
  libretro's thumbnails; they are looked up only when a game's page is opened, by `curl` on a
  thread of its own, and kept in the cache (`details/`), so the page shows them at once after,
  network or not (`src/details.rs`). The touch screen has Play, Favourite, Rating and Settings
  for this game (the menu, on its Game page; Back opens it too), and under them the game's
  achievement badges in a grid, the ones earned rimmed in gold; the one chosen goes on the box
  as a gold sticker with its points and description. With no set, the grid's place says why,
  or offers a sign-in. The menu's Game page no longer has the Achievements row.
- **Box art from the web for a game without its own.** When a game's page looks up its
  screenshots and the game has no `Imgs/<rom>.png`, libretro's `Named_Boxarts` is fetched too
  (by the ROM's file name, up to 60 seconds: they are large) and kept in DSlot's cache beside
  the screenshots (`details/<system>/<ROM name>/boxart.png`), never on the card. The shelf's
  box, the cart's label printed from it, the grid's cover, the system picker's blade and the
  back of the box use it where the card has none (`art::box_art`): the card's own art first,
  then the fetched, and the printed box only with neither. It shows as soon as it comes in.
- **L / R on the game page** step along Play, Favourite, Rating and Settings, round the row, as
  they turn the menu's tabs (from a badge, from the button above it); the hint line says so.
- **Y on the game page looks the game up again**: its screenshots, box art (where it has none
  of its own) and Wikipedia details, asked for afresh rather than waiting out the 90 days (a
  week for a miss), for a lookup that found nothing or the wrong game. A note on the box says
  *Looking this game up again…*, then what it found (or that there was no network); what the
  servers answer replaces what was kept, a screenshot they no longer have included, and the
  shelf's box is made again with new box art.
- **The Achievements page is just its rows**: the notes under them (that achievements are
  Casual for now, and which systems earn them) are gone. Softcore is called **Casual**, the
  service's own name for it, on the Mode row and in the points ("56 casual points").

## 0.4.1

- **Which systems earn achievements**, on the Achievements page under the Casual note: DS, Game
  Boy and Game Boy Color for now (DSperate and inkdot), with more coming. The shelf shows a set
  for any system RetroAchievements has, GBA's too; the note says where playing earns them.
- **The buttons come back when the gamepad does.** On nibOS the device's gamepad is
  nibos-input's, and a restart of it (an update of nibos-hw, or after a crash) makes a new one;
  the shelf kept the old one and heard nothing but the power key until DSlot itself restarted.
  It now opens a controller SDL reports as added and lets go of one reported gone (`ui/pads.rs`).

## 0.4.0

- **RetroAchievements**, an **Achievements** tab in the menu wherever there is `curl`: sign in
  with the username and password typed on the keyboard, see who is signed in and their points,
  and sign out. Achievements are softcore for now: hardcore (a Mode row) stays hidden until the
  service approves our emulators for it, and the page ends with a note saying so. DSlot keeps
  the token the service answers with (two lines, username and token, mode 0600, beside
  settings.toml), as soon as it answers, even if the menu has been closed meanwhile, and never
  the password, and hands it to every emulator it starts as `RA_TOKEN_FILE`, `RA_USERNAME` and
  `RA_HARDCORE` in the environment; DSperate reads them. A token the service refuses (the
  password changed on the site) is dropped and the page asks for a sign-in again. The service has no way to make an
  account from a device, so **Make an account** shows the sign-up page's address and a QR code
  of it for a phone. docs/retroachievements.md.
- **Achievements on the shelf**: signed in, the top screen shows the selected DS, GBA, GB or GBC
  game's standing with RetroAchievements under its details (*19 of 28 · 425 of 630 points*) and
  a bar of how far through the set the player is. The menu's Game page gets an
  **Achievements** row: A browses the list on the top screen, with each badge, title, points
  and description. Looked up by the ROM's hash as the service does it, by a worker off the
  shelf's thread that never holds up starting a game; the set and the unlocks are kept in the
  cache for six hours, for each player apart (and asked for again after the game is played),
  so it works offline and the service isn't asked on every selection; the badges are kept in
  `~/.cache/retroachievements/badges`, shared with the emulators. Signed out, nothing is shown;
  signing in or out looks the games up again for the new player; a sign-in the service no
  longer takes says to sign in again. An achievement found earned that an earlier lookup found not is kept with when
  (`unlocked_at`), for showing the latest later. The service's *Warning: Unknown Emulator*
  entry, which it adds to every set for an emulator it doesn't know, is left out of the list
  and the count.

## 0.3.2

- **System info**, on the System page: a read-only page of the device and what it is using,
  read again every second while it is up. The board and the nibOS release (version, edition,
  channel, build), the kernel, the uptime, the A/B slot running, DSlot's and nibkit's versions;
  the memory in use of the total, what is available, DSlot's own resident size, the GPU's
  shared memory, the file cache, the swap on zram and the kernel's CMA pool; the load, each
  core's frequency and governor, the temperatures, the GPU's frequency; the system, NIBOS-DATA
  and TF2 card file systems; the battery; and each panel's mode. A line whose source the
  device lacks is left out rather than guessed.
- **Date and time**, on the System page (nibOS): the time, the date and the time zone, and
  whether the time was set from the network (on a card that brings Wi-Fi up on demand it can
  be hours out after a power-off). **Sync now** brings Wi-Fi up if it is off and waits for the
  time servers; a picker sets the zone (region, then place), which nibOS keeps across updates;
  **Set by hand** takes the year, month, day, hour and minute with the D-pad and turns network
  time off until **Use network time** turns it back on. All through nibOS's `nib clock`, as
  root by sudo; DSlot never sets the clock itself.

## 0.3.1

- **A game that comes straight back says why.** When an emulator stops at once, with an error
  or a signal, the shelf shows a notice with the emulator's own last line ("DSperate stopped
  (exit 1): launch: ... ZIP NOT SUPPORTED") until any button, and logs it. When DSperate
  crashed, the notice prefers the line its crash handler writes ("dsperate: crashed (SIGNAL)
  after ...") to the runtime's "Bus error" after it. On nibOS the notice
  offers **Y: send a bug report**: `nib bug-report send` posts a scrubbed copy of the boot log
  to nibOS's bug tracker and the notice shows the report code to quote, or where the report
  was saved on the card when there is no network.
- **Notices from nibOS.** A file nibOS leaves under `/run/nibos/notice/` (its `root-verify`
  does, while the card was written badly) is shown the same way when the shelf comes up, or
  when it appears while the shelf is showing; once per run, again at the next start.
- **The TF2 card no longer slows the shelf** with the slot empty: nibOS mounts it when it
  appears, DSlot waits briefly for a card that is in but not mounted yet, and **Rescan games**
  reads the system's Roms folders again, so a card put in or taken out since power-on is picked
  up. nibkit 0.1.1.

## 0.3.0: the first public release

DSlot is a cart-and-box game shelf for the Anbernic RG DS+, on its stock OS (an app in
`Roms/APPS`) or on nibOS (its boot UI). What it has:

- **More systems**: Famicom Disk System (`FDS`), Master System (`SMS`), SG-1000 (`SG1000`), PC Engine (`PCE`),
  PC Engine CD (`PCECD`: cue sheets and CHDs), Neo Geo Pocket (`NGP`: `.ngp`, `.ngc`) and
  WonderSwan (`WS`: `.ws`, `.wsc`) and PlayStation (`PS`: `.m3u`, `.chd`, `.cue`, `.pbp`; a
  playlist's discs aren't games of their own), shown as boxes for now; nibcase plays them.
  docs/more-systems.md.

- **System cards**: a Top Trumps style card for each system on the top screen, in the system
  picker and on the menu's page for the console: name and maker, cited stats (released,
  launch price, sold, processor, memory, screen, games) with bars comparing the systems, a
  short history and a fun fact in DSlot's own words, and what is on your cards for it (games,
  time played, the game played longest). The facts are in `data/systems/`, one file each,
  with their source; every system DSlot has a page for has one (a test sees to it). Each
  has a photo of the console from Wikimedia Commons, by Evan Amos and public domain (the PC
  Engine CD's by RetroEditor, CC BY 4.0, cropped), credited on the card and in
  `data/systems/photos/CREDITS.md` (installed as `/usr/share/doc/dslot/photo-credits.md`).

- **System updates** (nibOS): the menu's Update page checks for an update, downloads it in
  the background (pause and resume), restarts into it, shows what's new, says once that it
  updated (or went back), and offers going back to the version before. nibOS's updater does
  the work. docs/nibos.md, "Updates".

- **No dark moment at the first launch after power-on**: with nibOS's splash up, DSlot now
  puts the same picture up in its own buffers before SDL starts, so what SDL puts back when it
  lets go of the screens for a game is DSlot's (it was the splash program's, gone by then:
  "Could not restore CRTC", a screen dark until DSlot's last frame).

- **Game folders made for you**: on a card with `Roms/` (or nibOS's own data card, `/mnt/roms`),
  DSlot makes the missing folder of every system an installed emulator plays, with a
  `_README.txt` saying what goes in it; slot cards get slot's systems under `Games/`. The
  list is nibkit's `Emulators::systems`. README, "Where DSlot keeps things".

- **Get more ports**: PortMaster's own tile heads the Ports page, there before any port is
  installed; the page's ports are looked for again when it exits. docs/ports.md.

- **Keyboard**: the caret follows the passphrase's dots, and the Delete key is drawn (it showed
  blank: the font has no backspace sign).

- **Cards set up for slot CFW**: each card's `Games/<system>/` is read beside `Roms/<system>/`
  (from nibkit, so the emulators see it too); a ROM in both is listed once, as the one in
  `Roms/`. Labels and `cart_shell.txt` as below. docs/slot-cards.md.

- **Cart Studio compatibility**: labels and shells made with slot CFW's Cart Studio work here.
  Its labels (`Labels/GBA/<rom>.png` at a card's root, after DSlot's own) are cropped to fit;
  `cart_shell.txt` sets a cart's outline (the Game Boy's notched or rounded shape), plastic
  colour and finish: solid, clear (the board seen through it) or glitter. The colour and finish
  work on every system's carts. The format is slot's. docs/artwork.md.

- **Licences**: DSlot is GPL-3.0-or-later (LICENSE, the FSF's text). Every crate built in
  was checked against it, for both builds (docs/licences.md), and their notices go with both
  packages as THIRD-PARTY.md (`tools/third_party.py`). CONTRIBUTING.md says what a change and
  a new dependency need.

- **Game & Watch pictures shown whole**: a unit's picture (usually a photo of the unit, tall or
  wide) is fitted inside its box over a blurred, dimmed copy of itself instead of being
  cropped. docs/game-watch.md.

- **Discs on the CD systems' shelves**: a PlayStation or PC Engine CD game is drawn as its
  disc rather than a cart, to a CD's dimensions (the spindle hole, the clear hub with its
  stacking ring, the mirror band, the printed face, the rim), the face printed with the
  game's `Labels/` art or its box art, or its name on a colour field; silver with a sheen
  that turns with the light, or a PlayStation disc's black. A game of several discs (.m3u)
  carries a badge saying how many. Discs go into a slot a little wider than themselves.

- **Wi-Fi and Bluetooth pages** in the menu (nibOS): the networks in range with their signal
  and whether they are saved, joining one (its passphrase typed on nibkit's on-screen
  keyboard: the D-pad and A, or a tap; B deletes, Y shows it, Start done), leaving it (A twice)
  and forgetting it (Y twice); Bluetooth on and off, the paired devices with their kind and
  battery to connect or disconnect (A) and forget (Y twice), and pairing a new one (looking,
  then A on it; a PIN or passkey typed, or a passkey confirmed with A or B). A tap on the
  status bar's Wi-Fi or Bluetooth icon opens its page. The work is done on a thread of its own,
  through nibkit's `net` (iwd and BlueZ over D-Bus).

- **Ports**: a page for PortMaster's ports in `Roms/PORTS` on either card, each named and
  pictured from its own folder (`port.json`, `gameinfo.xml`, a screenshot; JPEG pictures
  are read now too), started with PortMaster's runner, with how to quit on the bottom screen
  while one plays (docs/ports.md). Apps installed with the system
  (`/usr/share/dslot/apps`) are listed with the cards' ones, for PortMaster itself.

- **The shelf rests when left alone**: three seconds after the last touch, with nothing moving
  or loading, DSlot stops drawing frames and only looks for input (20 times a second),
  drawing again when something is pressed or touched, or when the status bar changes (the
  clock's minute, the battery, the radios). Idle on the shelf, the GPU no longer works at
  60 frames a second for an unchanging picture.

- **Where you left off, on the top screen**: when the selected game has a resume point, its
  picture sits inset over the corner of the box (the box as before), framed, with a
  "Resume" tag and when it was saved. It is the resume point the next start will use: the
  chosen emulator's (DSperate's, both screens one over the other; inkdot's; nibcase's, the
  chosen core's own).
  Read on a thread of its own as each game is shown and kept; one without a picture is left
  out.

- **The device's card** on the Apps page: what you are holding (the board's name, chip, CPU,
  GPU, memory, screens and storage, from `data/devices/<board>.toml`), its system and when
  that was built, and how it is now: the battery and battery protect, the room on each card,
  how long it has been on, the time played in all and the game played most, and the apps,
  with the one used last. The device is drawn, not photographed.

- **Wi-Fi and Bluetooth in the status bar**, after the battery: Wi-Fi off, connecting (its
  fan lighting up in turn) or connected with its signal, Bluetooth off, on or connected (with
  how many devices), read every few seconds by nibkit's `net` on a thread of its own. Left out
  where the system doesn't say (the stock OS, a desktop).

- **Pages for the systems you have games for, and a system picker**: a system's page (and
  tab) is there only when it has games on a card (System → Show systems with no games puts
  them all back); a page appears with its first game and goes with its last. When the tabs
  don't fit beside the heading, the strip scrolls to the page's tab, faded at an end with more
  past it, and the heading keeps its room. Select (tapped alone; with Start it still leaves)
  opens the system picker: a blade across the touch screen for each system, the chosen one
  tall, with the system's name, its games, when one was last played and a cart, the others
  thinner slices; art from your own games where they have box art. Up / down or a finger
  scroll through them.

- **Carts drawn after real ones**: the Mega Drive, Game Gear and Lynx carts, and the SNES's in
  both shapes, US and Super Famicom, measured from photos of real carts, with labels laid out
  as each system's are (no logos or trademarks). The SNES page's **SNES carts** setting picks
  the shape: by the game's region (the default), US, or Super Famicom.

- **Game & Watch page** (`GW`: `.mgw`, or zipped): units named after their files, landscape
  boxes, and no carts, so its page is boxes alone as the apps' is (the Carts view shows the
  grid). Generated boxes on short landscape covers keep their title on them. docs/game-watch.md.

- **Mega Drive, Game Gear and Atari Lynx shelves** (`MD`, `GG`, `LYNX`): their own cartridges,
  portrait covers, filename-based titles and shared system/game settings and saves. nibcase
  registers BlastEm, Gearsystem and Handy commands/options automatically. Lynx needs the
  user's BIOS. Optional stock-core descriptions are detected by their installed libraries.
  docs/sega-lynx.md.

- **NES shelf** (`Roms/FC`: `.nes`, `.unf`, `.unif`, `.zip`): grey front-loading cartridges,
  portrait covers, filename-based titles and shared `FC` settings/saves. nibcase's separately
  packaged Nestopia integration registers its launch command and options automatically.
  The stock build detects an installed Nestopia/RetroArch core. docs/nes.md.

- **SNES shelf** (`Roms/SFC`: `.sfc`, `.smc`, zipped, including `.smc.zip`): a grey
  Super Famicom-style cartridge, landscape box art and covers that keep their proportions in
  the grid. LoROM, HiROM and ExHiROM metadata, with copier headers and stored/deflated ZIPs;
  game codes and publishers where available, filenames when a header is unreadable. Saves
  and settings use `SFC`. A stock Snes9x description and a tested example contract for the
  nibcase frontend on nibOS (packaged separately). docs/snes.md.
- **Automatic emulator discovery while running**: installed/removed descriptions, available
  programs/cores and changed option definitions refresh the shelf and settings within about
  two seconds; installing a nibcase core needs no DSlot configuration.

- **Momentum scrolling**: a flicked grid or list keeps scrolling and slows to a stop, as iOS's
  lists do; dragged past either end it stretches and springs back, and a touch stops it. The
  selection stays on screen as it moves, so the top screen shows each box as it passes.

- **Games noticed as they come and go**: games copied onto a card or deleted from it while
  DSlot runs, and cards put in or taken out, update their page about a second after the copy
  ends (inotify on each console's folders and udev's list of cards), on a thread of its own;
  the selection, page and view stay as they were. **Rescan games** on the System page looks
  at every page again.

- **List views**: View can be a list of names, the selected game's box on the top screen as
  it moves, or the same with a small box beside each name (`list`, `list-covers`). Up and down
  step a game, left and right a letter; tap a name to pick it, again to play.

- **nibOS build** (`--features nibos`, `packaging/nibos`): both panels through KMS with a
  window and GLES renderer each; on screen about five seconds after the power button, from a
  skeleton of the shelf put straight on the CRTCs, or behind nibOS's power-on splash; Mesa
  loaded and the games listed while SDL starts; controllers and audio opened after the first
  frame. An Arch package with its systemd unit and the splash's picture. docs/nibos.md.
- **Pages**: DS and GBA, L / R or the tabs to switch, each page remembered. GBA games from
  `Roms/GBA` (`.gba`, or zipped), named from the file the way No-Intro names them, game code
  and maker from the header; a GBA cart drawn after a real one, going into slot-2. Game Boy
  and Game Boy Color too (`Roms/GB`, `Roms/GBC`: `.gb` or `.gbc` in either, or zipped, also
  as `.gbc.zip`), their carts drawn after real ones: the grey DMG cart with its notch, and the
  clear Color-only one with the board showing through. A page appears only when an emulator
  for its console is installed.
- **Emulators describe themselves** in small TOML files (docs/emulators.md); each console's
  default is the highest priority installed, and the menu has an **Emulator** row where a
  console has more than one. DSlot makes the saves folder and runs the emulator itself.
  `settings.ini` and DSperate's launcher's `launch.conf` are read into settings.toml once.
- The stock build ships descriptions for NNDDSS (still the default for DS), DraStic, and
  RetroArch's mGBA, gpSP and Gambatte through the stock menu's own launcher.

- **Settings at three levels**: the system, a console, a game, each winning over the one
  before, in one file, `settings.toml` (docs/settings.md). The menu has a page for each, Game,
  the console's and System; a row's values start with what it inherits (*Use System
  (Carts)*, muted), so setting and unsetting it is the same left and right (or Y).
  Shelf look per console (a GBA page in carts beside a DS one in the grid), button repeat.
- **Emulators declare their options** (type, values, default, the words they become) and
  **actions** on a game's files in their description file; DSlot shows them as settings for a
  console and a game and passes them when the game starts (one set nowhere isn't passed, and
  shows as *DSperate's own*: the emulator's own setting holds), and offers the actions (and
  its own Delete saves) on the Game page, each asking first. An emulator may set its own
  options in DSlot's settings from its own menu: DSlot reads them again when a game exits.
- **The device's settings** on nibOS: volume, each screen's brightness (as you move the slider,
  saved when you let go; read from nibOS, so they follow Menu + volume; volume follows the
  volume keys), Wi-Fi, the battery
  (charge, time to full) and battery protect (stop charging at 80%). The status bar's battery
  is nibOS's count too, with an icon: its level, and a bolt while charging or a plug on the
  charger when it isn't. A 12-hour or 24-hour clock, shared with every app on nibOS. The speaker amps' tuning, nibOS's or
  the official one imported from your own stock firmware, with Restart now to apply it.
- `dslot --launch ROM` starts one game as the shelf would, without the shelf.
- **Apps tab**: apps in `Roms/APPS` on either card (a folder with an `app.toml`; on the stock
  OS the stock menu's scripts too) get a page of their own when there are any, a grid of their
  icons, started like emulators. docs/apps.md.
- **Power menu** on nibOS (a long press of power): Sleep, Restart, Power off, Cancel. A game
  running is put away first (SIGTERM: it saves its auto-resume state and quits), and Cancel or
  waking up starts it again where it was.
- **Screensaver**: left alone for a while (System page: off, 1, 2, 5, 10 min; 2 by default),
  the screens dim and the DSlot logo bounces round both of them as one tall space, a new colour
  at each wall. Any button, touch or the lid wakes it without acting; the brightness goes back
  exactly as it was. The logo drifts (90 x 66 units a second), placed to a fraction of a pixel so it glides,
  and is drawn 15 times a second, nothing drawn in between; **Screensaver style** picks
  Animated, Still (moved once a minute) or Screens off. Measured on the DS+ (RK817 coulomb
  counter, 90 s, discharging): idle shelf 516-536 mA; under the screensaver, 25 fps 488,
  8 fps 474, 5 fps 474, still 474, screens off 467-473; later, at 15 fps (the default) 481
  against still 495 in the same sitting. Its drawing costs nothing measurable over a still logo.

- **Status bar icons, also over the menu**: Wi-Fi's and Bluetooth's icons stay in the bar
  while the menu is open, changing as its pages change them. Off, an icon isn't shown; on with
  nothing connected, it is dim; connected, bright (Wi-Fi with its signal, Bluetooth with a dot
  or how many). They change at once when a radio is switched, joined or connected (nibkit's
  `net::refresh`), not at the next read a few seconds later.
- **Game & Watch as MAME sets, and inktick**: `.7z` and `.zip` sets on the G&W page, named
  by the list inktick installs (title, and model, year and series under it). An emulator can
  list the only games it plays (`[games]` in its description, nibkit): inktick takes the
  units it lists, nibcase-gw the rest. inktick's resume pictures show on the top screen.

- **The Wi-Fi switch is Wi-Fi's alone** (nibOS's `nib wifi`): switching Wi-Fi off leaves
  Bluetooth (and headphones) connected, and the choice is kept across restarts.
- **Bluetooth on and off whatever state BlueZ is in**, with no D-Bus errors on screen: off
  right after the Wi-Fi switch restarted BlueZ used to say "UnknownObject ... Set".

- **No text console between the shelf and a game** on nibOS: DSlot keeps the display open and
  its last frame on screen while a game starts and after it ends.

- **Held directions repeat** everywhere: the D-pad and now the left stick, up, down, left and
  right, on the carts, the cover grid, the menu and the power menu (after the Button repeat
  setting's delay, then steadily, twice as fast once held 1.5 s; never queued up behind).
