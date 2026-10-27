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
| Sign in | A asks for the password (shown as dots; Y shows it), then signs in. The row says *Signing in…*, then the title line says who is signed in, or why not: the service's own words for a wrong password, *No network* when it can't be reached. |
| Mode | Softcore (the default) or Hardcore: a System setting, `achievements` in settings.toml. See below. |
| Make an account | The sign-up page's address and a QR code of it, for a phone. B comes back. |

Signed in:

| Row | |
|---|---|
| Signed in as | The username as the service spells it, with the points the service reported (hardcore, and softcore when there are any). |
| Sign out | A, and A again to confirm: the token file is removed. The username stays typed for next time. |
| Mode | As above. |

Each time the menu turns to the tab with a sign-in kept, DSlot asks the service once whether the
token still stands (the same request, with the token in place of the password) and shows the
points it answers with. A token the service refuses (the password was changed on the site, say)
is dropped, and the title line asks for a sign-in again. With no network the sign-in kept stands
as it is.

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

The sign-in is `r=login2&u=<user>&p=<password>` posted to `https://retroachievements.org/dorequest.php`
by `curl` (`--data-binary @-`: the form goes on curl's standard input, never on its command line),
with the user agent `DSlot/<version>`. A success answers with `User` (the username as the service
spells it), `Token`, `Score`, `SoftcoreScore` and `Messages`; a refusal with `Success: false` and
an `Error` line, which the tab shows.

What is kept, and where:

- `retroachievements.token`, beside settings.toml (`~/.config/dslot/` on nibOS, the app's folder
  on the stock OS): two lines, the username then the token, mode 0600. It is the format DSperate's
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

When the player is signed in, every game DSlot starts gets three environment variables, whatever
its emulator (`[env]` in the emulator's description is kept as well):

| Variable | |
|---|---|
| `RA_TOKEN_FILE` | The token file's path. |
| `RA_USERNAME` | The username, for an emulator that reads a one-line token file. |
| `RA_HARDCORE` | `1` when Hardcore is set, else `0`. |

Signed out, none of them is set. An emulator that knows nothing of them is unaffected. The
emulator reads the file itself, signs in with the token (`login2` with `t=`, or rcheevos'
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
