# RetroAchievements

The **Achievements** tab of DSlot's menu signs the player in to
[RetroAchievements](https://retroachievements.org) once, for every emulator on the device. It is
there wherever DSlot finds `curl` (nibOS; a desktop preview too). DSlot itself awards nothing: it
keeps the sign-in and hands it to the emulator when a game starts. What each emulator does with
it is the second half of this page.

## The tab

Signed out:

| Row | |
|---|---|
| Username | A types it on the keyboard (nibkit's, the one the Wi-Fi page uses). |
| Sign in | A asks for the password (shown as dots; Y shows it), then signs in. The row says *Signing in…*, then the title line says who is signed in, or why not: the service's own words for a wrong password, *No network* when it can't be reached. The token is kept by the request itself, before the page hears of it, so a sign-in stands even if the menu is closed while it is on its way. |
| Mode | Casual (the service's name for softcore; the default) or Hardcore: a System setting, `achievements` in settings.toml. Not shown yet: see [Hardcore, not yet](#hardcore-not-yet). |
| Make an account | The sign-up page's address and a QR code of it, for a phone. B comes back. |

Signed in:

| Row | |
|---|---|
| Signed in as | The username as the service spells it, with the points the service reported (hardcore, and casual when there are any). |
| Earned achievements | Every achievement earned, a row each in place of the page's rows: its title, then the game and its points; the title line has the highlighted one's description, or the count of them all. The latest DSlot saw earned come first, then the rest by game. It lists what the shelf's lookups have kept for the player (`achievements/<user>/` in DSlot's cache), so a game never shown on the shelf here isn't in it, and it reads no network. B comes back. |
| Sign out | A, and A again to confirm: the token file is removed. The username stays typed for next time. |
| Mode | As above (not shown yet). |

## Hardcore, not yet

Hardcore unlocks count only from emulators the service has approved, and it approves an
emulator only once it has been public for six months and enforces the hardcore rules (no state
loads, rewind, slow motion, frame advance or cheats; a resume drops to softcore; switching to
hardcore resets the game). See the service's
[hardcore compliance requirements](https://docs.retroachievements.org/general/hardcore-compliance-requirements.html).
Until ours are approved, DSlot offers softcore alone: `HARDCORE_OFFERED` in
`src/achievements/mod.rs` is off, so the Mode row is hidden and `RA_HARDCORE` is always `0`,
even with `achievements = "hardcore"` in settings.toml. Turning it on brings both back.
Meanwhile the service adds a *Warning: Unknown Emulator* entry ("Hardcore unlocks cannot be
earned using this emulator", id 101000001 and up, earned already) to every set it sends us;
DSlot leaves it out of the list and the count, as DSperate and inkdot leave it out of their
notices.

Each time the menu turns to the tab with a sign-in kept, DSlot asks the service once whether the
token still stands (the same request, with the token in place of the password) and shows the
points it answers with. A token the service refuses (the password was changed on the site, say)
is dropped, unless the file has changed since the check began, and the title line asks for a
sign-in again. Only a refusal of the sign-in does that (the service's codes
`invalid_credentials`, `expired_token` and `access_denied`, as rcheevos reads them); the service
failing otherwise, or no network, leaves the sign-in kept as it is.

Signing in, signing out and a token dropped are each told to the shelf, which lets go of what it
looked up for the last player and looks the selected game up again for the new one.

## On the shelf

With the player signed in, the top screen says, under the selected game's details, how it
stands with the service: *Achievements*, then *19 of 28 · 425 of 630 points*, then a bar of how
many of the set are earned. A game the service doesn't know says *Not on RetroAchievements*;
one still being looked up, *Looking up…*. Signed out, nothing is shown: not even what was kept
for the last player. A sign-in the service no longer takes (refused on a lookup, with nothing
fresh kept for the game) says *Sign in again in the menu*.

To see them all, X opens the game page: the touch screen has the set's badges in a grid under
its buttons, the ones earned rimmed in gold, the rest dimmed, with the standing above them. The
D-pad (or a tap) picks one, and it goes on the back of the box on the top screen as a gold
sticker: its badge, title, points, whether it is earned, and its description. With no set to
show, the grid's place says why (*Looking up achievements…*, *Not on RetroAchievements*), or
offers **Sign in to RetroAchievements**, which opens the menu's Achievements page. The badges
are made into textures for both screens.

How it is known, all off the shelf's thread, by a worker of its own (`achievements/lookup.rs`).
It looks up the game asked about last first: browsing quickly past a row of games looks up the
one stopped at, and the ones passed over are asked about again when they are shown again. It
reads the sign-in afresh for each lookup. It is a thread of its own, not tied to the shelf:
starting a game never waits for a request to the service to finish.

- **Which game.** The ROM is hashed by rcheevos' own rhash, through nibkit's `nib-cheevos`
  (`hash`), the same code the emulators use, so the shelf and the game always agree: every
  console's rule (the MD5 of the file for Game Boy, Game Boy Color, Game Boy Advance, Mega
  Drive and most others; headers left out for NES, FDS, SNES, PC Engine and Lynx; for a DS game
  its header, its two pieces of boot code and its icon/title block; for a PlayStation or PC
  Engine CD disc, from its cue sheet or playlist, what rhash reads of it). A zip is its first
  entry with one of the console's extensions. CHD images and Game & Watch units have no hash
  yet (rhash reads no CHD, and has no Game & Watch rule), so the shelf shows nothing for them.
  The shelf looks up any console's game, whether or not an emulator here earns achievements
  for it: it shows the set the service has. The hash is kept in `cache/achievements/hashes.tsv` against
  the file's path, size and date, so a ROM is read once. The index is anyone's, whoever is
  signed in; it is written beside itself and renamed into place, and a line it can't read is
  passed over, not the end of it.
- **The set, and what is earned.** Requests of the emulator API with the sign-in kept, each
  built and its answer read by rcheevos (`rc_api_*`, through `nib-cheevos`' `api`), the form on
  `curl`'s standard input as for signing in: `achievementsets` with the hash (the game's sets,
  the core set and any bonus set that goes with it: each achievement's title, description,
  points and badge name; the unofficial ones and the service's "Unknown Emulator" warnings are
  left out), then `unlocks`, softcore and hardcore, for each set. The answer is kept in
  `cache/achievements/<user>/<hash>.json`, the player's own (another account's unlocks are
  never shown, nor taken for what changed since), with when it was fetched, and stands for six
  hours: browsing the shelf asks the service nothing in between, and with no network the shelf
  shows what was kept for the player, however old. A game just played is asked about again
  next time it is selected, since it may have earned some (found by the hashes index alone: the
  ROM is never read between the game and the shelf). The service not knowing the ROM is kept
  too, so it isn't asked again every time.
- **When each was earned.** `unlocks` says which, not when, so DSlot dates what it sees change:
  an achievement a lookup finds earned that the one before found not gets that lookup's time
  (`unlocked_at` in the cached set, Unix seconds), which, since a game just played is asked
  about again as soon as it is selected, is close to the end of the session that earned it.
  One already earned the first time the game is looked up has no date. Nothing shows the dates
  yet. The emulators know better (rcheevos gets every unlock's time when a game starts), so
  DSperate writing them where DSlot can read them is the way to date the rest.
- **The badges.** `https://media.retroachievements.org/Badge/<name>.png`, fetched once each
  into `~/.cache/retroachievements/badges/` (`$XDG_CACHE_HOME` when it is an absolute path;
  DSlot's own cache folder when there is no home: `nib-cheevos`' `paths`, which every emulator
  uses too) and read from there after. The folder is shared: DSperate keeps
  the badge of an unlock it shows there, and the other emulators will. A badge not in yet shows
  as a faint square.

`cache/` is DSlot's cache (`~/.cache/dslot` on nibOS): safe to delete, remade as games are
selected.

## The account question

There is no way to make an account from the device. The service's two APIs are:

- the **web API** (`retroachievements.org/API/`, documented at api-docs.retroachievements.org):
  reads game, user and achievement data, authenticated with an account's own web API key;
- the **emulator API** (`dorequest.php`, the requests rcheevos makes): `login2` signs in with a
  username and a password or a token, then the session, game-identification and award requests,
  all of which need a signed-in user.

Neither registers a user. Sign-up is the website's `createaccount.php` form, behind a bot check,
with an email confirmation after it. So the tab's **Make an account** row shows the address,
`retroachievements.org/createaccount.php`, with a QR code of it (the `qrcodegen` crate, drawn
under the row) for the player to open on a phone, make the account, and come back to sign in.
There is no shorter official link.

## Credentials

The sign-in is `r=login2&u=<user>&p=<password>` (as rcheevos' `rc_api_init_login_request` builds
it) posted to `https://retroachievements.org/dorequest.php` by `curl` (`--data-binary @-`: the
form goes on curl's standard input, never on its command line), with the user agent
`DSlot/<version> (nibOS; linux aarch64) rcheevos/12.4` (`nib-cheevos`' `ua`). A success answers with `User` (the username as the service
spells it), `Token`, `Score`, `SoftcoreScore` and `Messages`; a refusal with `Success: false` and
an `Error` line, which the tab shows.

What is kept, and where:

- `retroachievements.token`, beside settings.toml (`~/.config/dslot/` on nibOS, the app's folder
  on the stock OS): two lines, the username then the token, mode 0600, written beside itself and
  renamed into place so that it is never half there. It is the format DSperate's
  `--cheevos-token` reads. On nibOS the home folder lives on the data card, so it outlives an
  update of the root the way settings.toml does.
- The **password is never written**: not to the token file, not to settings.toml, not to the
  journal, and it leaves DSlot's memory when the request has gone.
- `achievements = "hardcore"` (or `"softcore"`) in settings.toml's `[system]` table.

Nothing of the sign-in goes into a log or a bug report: DSlot's journal lines say `signed in as
<user>`, `signed out`, and the service's refusal line, nothing more. `nib bug-report` sends a
scrubbed copy of the boot log alone: it reads neither settings.toml nor the token file. Sign
out before handing the device to someone else.

## How a game gets it

When the player is signed in, a game DSlot starts gets three environment variables when its
emulator's description says it earns achievements for the game's system (`[achievements]`,
docs/emulators.md; `[env]` in the description is kept as well). An emulator without the table,
or a game of a system it doesn't name, gets none of them: the token goes only where it is used.

| Variable | |
|---|---|
| `RA_TOKEN_FILE` | The token file's path. |
| `RA_USERNAME` | The username, for an emulator that reads a one-line token file. |
| `RA_HARDCORE` | `1` when Hardcore is set and offered, and the description says `hardcore = true`; else `0` (always `0` for now). |

Signed out, none of them is set. The emulator reads the file itself (a Rust one with
`nib-cheevos`' `launch::Launch::from_env`, which reads both token-file formats), signs in with the token (`login2` with `t=`, or rcheevos'
`rc_client_begin_login_with_token`), identifies the game and awards. The password is not there to
pass. The mode is a wish: an emulator that cannot play hardcore (DSperate today) says so in its
log and plays softcore, and the service credits softcore.

## The emulators

Clean room throughout: each emulator reads the RetroAchievements documentation
(docs.retroachievements.org) and rcheevos' own API, never another emulator's source.

### DSperate (DS, GBA)

Done. DSperate already had `rc_client`-based support (`src/cheevos/`, vendored rcheevos, libcurl
through `dlopen` at run time): hashing a DS or GBA ROM as the service does, a session, unlock
toasts, an achievements list and account page in the pause menu, a stored sign-in of its own, and
`--cheevos-token FILE`. What the integration added:

- `RA_TOKEN_FILE` stands in for `cheevos.token_file` when the config names none, and turns
  achievements on when `cheevos.enabled` is not set; `RA_USERNAME` fills in a username the file
  lacks. `--no-cheevos` or `enabled = false` still win.
- `RA_HARDCORE=1` is noted in the log (`cheevos: hardcore was asked for ...; DSperate plays
  Casual`). DSperate forces hardcore off in `rc_client` because it allows save states, rewinds and
  cheats; hardcore would need those locked while a set is loaded, and the service's approval.
- **The unlock toast.** When rcheevos fires an achievement-unlocked event, DSperate shows a
  toast for four seconds in the corner of the top screen: the badge, *ACHIEVEMENT UNLOCKED*,
  the title with its points, and the description. The badge is fetched (or read from the shared
  badges folder above) and decoded by DSperate's own PNG reader on its RetroAchievements worker
  thread, which runs at ordinary priority, never on the emulation thread; the toast waits up to
  a second and a half for it and then shows without. The toast is DSperate's own drawing
  (nibkit's shared pause menu has no overlay for a game that is playing): nibcase and the others
  draw their own the same way, from the same badge files.
- nibOS's package (`packaging/arch/PKGBUILD` in the fork) builds with `DSPERATE_CHEEVOS=ON` and
  depends on `curl` for `libcurl.so.4`.

### nibcase (libretro cores: NES, SNES, Mega Drive, Master System, Game Gear, PC Engine, Lynx, Neo Geo Pocket, WonderSwan, PlayStation, Game & Watch)

To do. The pieces the service needs are what libretro cores already expose, so the work is in the
frontend, once for every core:

- **Identity**: rcheevos' `rc_hash` knows each of these systems' hashing rules (MD5 of the file
  for most; headers skipped for NES, SNES, PC Engine and Lynx; a track hash for PlayStation
  discs). nibcase hashes the file it was given before handing it to the core.
- **Memory**: `RETRO_ENVIRONMENT_SET_MEMORY_MAPS` from the core, or failing that
  `retro_get_memory_data`/`size` for RAM and SRAM, fed to rcheevos' `rc_libretro_memory_init`,
  which maps the service's address space for the console onto the core's regions and serves
  `rc_libretro_memory_read`. Cores that give no map at all (a few of the older ones) get no
  achievements; nibcase says so once in its log.
- **Session**: `rc_client` with a `curl` transport (nibcase is Rust: `rcheevos` through a small
  `cc`-built binding, or a pure-Rust client of `dorequest.php`, which is a handful of
  form-encoded requests; the binding keeps the condition evaluator, which is the bulk of the
  work, as the service's own). Sign in with the token file, `rc_client_begin_load_game` with
  the hash, `rc_client_do_frame` after each frame, a toast on
  `RC_CLIENT_EVENT_ACHIEVEMENT_TRIGGERED` through nibkit's menu toasts.
- **Hardcore**: nibcase can honour `RA_HARDCORE=1` properly, since it owns save states and
  rewind: with hardcore on, loading a state and rewinding are refused while a set is loaded, and
  the pause menu says why. Approval for hardcore unlocks is per emulator, by the service's staff,
  and needs the user agent `nibcase/<version>` registered with them; until then the service
  counts the unlocks as softcore.

### inktick (Game & Watch)

Possible, later. The service lists Game & Watch as a console (rcheevos' `RC_CONSOLE_GAME_AND_WATCH`)
with sets made against the `gw` libretro core's memory layout (the game's `.mgw` package,
hashed as a file). inktick plays the same packages, so identity is the file's hash as `rc_hash`
does it; what it would need is to lay its CPU's RAM out as that core does, so the sets'
addresses mean the same thing. That is a read of the service's console documentation and some
testing against a set, not a big piece of code, but it waits until nibcase has the shared Rust
client to reuse.

### inkdot (Game Boy, Game Boy Color)

Not started; what it would need:

- **Identity**: the MD5 of the ROM file, which is all the service wants for GB and GBC.
- **Memory**: a read callback over the service's Game Boy address space, which is the CPU's bus
  (`$0000`–`$FFFF`) followed by cartridge RAM and, for GBC, the switchable work-RAM banks, laid
  out as rcheevos' console table (`rc_consoles.c`) defines them. inkdot has the bus; the extra
  regions are its cartridge's RAM and its WRAM banks, read straight from the arrays.
- **Session**: the same client as nibcase (the shared Rust piece), driven once a frame.
- **Hardcore**: as nibcase: refuse state loads and rewind while hardcore is on, or play softcore.

The order, then: nibcase next (most systems, the client written once), inkdot after it with that
client, inktick last.
