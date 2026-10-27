# DSlot on nibOS

On nibOS DSlot is the boot UI: `dslot.service` starts it from `sysinit.target`, in place of the
tty1 login shell, and it is on both screens about five seconds after the power button. This is
how, and what the nibOS build does that the stock one doesn't.

## The package

`packaging/nibos/PKGBUILD` builds the `dslot` package (docs/building.md):

| | |
|---|---|
| `/usr/bin/dslot` | the program, built with `--features nibos` |
| `/usr/lib/systemd/system/dslot.service` | the unit (`packaging/nibos/dslot.service`) |
| `/usr/share/dslot/emulators/` | where emulators' packages put their descriptions (docs/emulators.md) |
| `/var/lib/dslot/splash` | the power-on splash's picture (`dslot --write-splash`) |

What nibOS itself provides (its `rootfs/dslot/`): the user DSlot runs as, in a drop-in,
`/etc/systemd/system/dslot.service.d/user.conf`:

```ini
[Service]
User=<user>
Group=<user>
Environment=XDG_RUNTIME_DIR=/run/user/<uid>
```

and the polkit rule that lets that user power off, `nib boot-ui on|off`, expedited RCU and the
TF2 card's mount. Without the drop-in the unit would run DSlot as root, which DSlot refuses.

## Settings

`/etc/default/dslot`, read by the unit:

| | |
|---|---|
| `DSLOT_ARGS` | arguments, e.g. `--roms DIR` for other Roms folders (default `/mnt/roms/Roms` and `/mnt/ds/Roms`) |
| `DSLOT_KMS=0` | no skeleton: SDL takes the screens straight away |
| `DSLOT_PRELOAD=0` | leave loading Mesa to SDL (by default it starts on a thread at once, ~0.15 s sooner to the shelf) |
| `DSLOT_WAIT_FOR` | a mount point to wait for (up to 3 s) before listing games |
| `DSLOT_SWAP=1` | the screens the other way round |
| `DSLOT_FPS=1` | log the frame rate every 300 frames |
| `DSLOT_TOUCH_DEV` | the touch panel's event device, if DSlot doesn't find it by name |

## The power menu

A long press of power asks DSlot for its power menu: nibOS sends dslot.service's main process
SIGUSR1 (nibkit::power). On the shelf the menu opens at once: Sleep, Restart, Power off,
Cancel. During a game DSlot first puts the game away (SIGTERM: it saves its auto-resume state
and quits, docs/emulators.md), then opens the menu; Cancel, or waking from Sleep, starts the
game again where it was. Sleep, restart and power off go through logind (`systemctl suspend`,
`reboot`, `poweroff`), which nibOS's polkit rule lets DSlot's user do.

## Bug reports from the shelf

When a game comes straight back (the shelf's "The game stopped" notice, README.md "A game
does not start"), the notice offers **Y: send a bug report** where nibOS's
`/usr/lib/nibos/nib-bug-report` is installed (never on the stock OS or a desktop preview).
DSlot runs `nib bug-report send --why "<the notice's line>"` on a thread (bugreport.rs): the
command saves a scrubbed copy of this start's log on the card as `nibos/bug-report-DATE.txt`
(its `saved:` line) and posts it to the bug tracker. The shelf says "Sending the report..."
while it runs (up to a minute), then "Sent. Report code XXXXXXXX" (the eight characters after
`sent: report code`, shown large to copy by hand), "No network. Saved on the card as ..."
(exit 4) or "Could not send. Saved on the card as ..." (exit 3, no tracker configured, or 5,
the send failed). The command's output goes to the journal. Any button then closes the notice.

## Network info

The Wi-Fi page shows the IPv4 address under the network joined (**Address**), and opens
**Network info** (its row, or **Y** on any row but a saved network, where Y forgets): a
read-only page of what a player is asked for when something won't connect, so support needn't
guess whether they were on Wi-Fi at all (`src/ui/netinfo.rs`). Its lines: Wi-Fi (off, on and
not connected, on and connected), the network with its signal in dBm and band, the address with
its prefix, the gateway, the DNS servers, the device's name on the network (`<hostname>.local`,
what to ssh to), the Wi-Fi MAC, and three reachability checks: the internet
(`one.one.one.one` on 443; when its name fails but `1.1.1.1` answers, "DNS failed"), the update
server (`api.github.com`) and the bug tracker (the host out of `/etc/nibos/bug-report.conf`'s
DSN, never its key; "Not set" without one). Each says "Reachable", "No route" or "DNS failed";
when the internet check finds no route, the other two say so without trying. The DNS line is
shown only while there is a route (without one, resolved lists just its fallback servers).
The title line sums it up ("Connected to Endless", "Wi-Fi is on, not connected", "Wi-Fi is
off"), and when Wi-Fi is off or no network is joined, the address, gateway, DNS and network
lines say that rather than going blank.

Nothing here needs root or a daemon: the facts are read on a thread every 2 s while the Wi-Fi
page is up, from `nib wifi status` (`nibkit::device::wifi`), `ip -j addr`, `ip -j route`,
iwd's `iwctl station wlan0 show` (or `iw dev wlan0 link` without iwctl),
`/run/systemd/resolve/resolv.conf` and `/etc/hostname`. The checks run on a thread of their own
when the page opens and again on **A** (each a name lookup and a TCP connect to port 443, 4 s at
most), so a slow one never holds the page. **B** goes back to the Wi-Fi page. The parsers have
unit tests fed with the commands' captured output. On the stock OS there is no Wi-Fi page and
none of this is built in.

## Date and time

The System page's **Date and time** row (nibOS, where `/usr/lib/nibos/nib-clock` is
installed; never on the stock OS) shows the time and zone at a glance and opens the page
(`src/ui/clock.rs`): the time to the second (12- or 24-hour as the Clock row says), the date
in words, the zone with its offset, and **Time source**: "Set from the network (last at
10:47)", "Not set from the network yet" (a warning: on the DSlot edition Wi-Fi comes up on
demand, so after a power-off the clock runs from the hardware clock until something brings the
network up), or "Network time is off: set by hand". The title line says the same. Then:

- **Use network time**: systemd-timesyncd on or off (`nib clock ntp on|off`); turning it on
  with the network up waits up to 30 s for the servers.
- **Sync now**: `nib clock sync` brings the network up as `nib bug-report send` does (`nib net
  up`, `nib wifi on`, up to 20 s for a route), restarts timesyncd and waits up to 30 s for a
  reply; the row says "Set from the network", "No network: turn Wi-Fi on, or set the time by
  hand" or "The time servers didn't answer: try again".
- **Time zone**: a picker, regions first (UTC, Africa, America, Antarctica, Arctic, Asia,
  Atlantic, Australia, Europe, Indian, Pacific: `timedatectl list-timezones` without the
  legacy names), then the region's places with the current one ticked; left and right jump ten
  at a time. A sets it (`nib clock zone`), which nibOS also keeps on its state partition so an
  update doesn't lose it.
- **Set by hand**: year, month, day, hour and minute rows stepped with left and right (the day
  kept within the month), **Set the clock** applies them (`nib clock set`), which turns network
  time off so it isn't put straight back.

`status` is read as any user on a thread every second while the System page is up; the
commands run as root through `sudo -n` (nibOS's `sudoers.d/nibos-clock` lets the wheel group)
on a thread of their own, the row saying "Working…" until they answer; their output goes to
the journal. nibOS writes the hardware clock after a set or a sync. B goes back a level, then to
the System page. The parsers, the zone grouping and the fields' stepping have unit tests.

## Notices from nibOS

Each file under `/run/nibos/notice/` (a tmpfs) is a notice for the shelf: its text, wrapped,
in the same box as the stopped notice ("A notice from nibOS"), until any button. nibOS's
`root-verify` leaves one there at every start while the card was written badly, for instance.
The folder is read when the shelf comes up and every three seconds while it shows (notices.rs);
a file is shown once per run of DSlot, by name and modification time, and again at the next
start.

## Updates

The menu's **Update** page (where nibOS has its updater, `nibos-update@.service`) shows the
version and edition running and the update channel (Stable or Beta), and takes an update from
checking to installing:

- **Update** checks; with one there, it reads "Download (N MB)" and the line above says which
  version. The download runs in the background (the shelf and games carry on); A pauses it and
  resumes it. Once it is in, **Restart to update** (A twice) restarts into it.
- **What's new** shows the update's notes. DSlot keeps them as it restarts, for after.
- After an update DSlot opens on the Update page once, saying "Updated to nibOS Y" (or, if it
  didn't start and the device went back, which version it is on). **Go back to the previous
  version** (A twice) is offered while the updater says the other slot holds a good one.
- An error (no network, the battery low and not charging, a damaged download) is said in the
  line above, in DSlot's words for the updater's `code`; **Try again** repeats the step that
  failed. The updater's own `detail` is shown only when it is plain words, never a URL or a
  tool's output (that is in the journal), and a long line goes on to the rows below. No
  releases published for the device yet (`none_published`) reads "No updates published yet", not as an
  error. The codes: nibOS's docs/design/updates.md.

The work is nibOS's: DSlot starts the updater's steps (`systemctl start
nibos-update@<step>.service`, allowed for the wheel group) and reads
`/run/nibos/update/status.json`; it never writes a partition and never runs as root
(`src/update.rs`). `DSLOT_UPDATE_STATUS=<file>` shows the page from a status file of your own,
in a desktop preview too.

## The device's settings

The System page of the menu changes the device itself, through what nibOS offers its user:

| | How |
|---|---|
| Volume | `pactl` on the default sink (PipeWire's), as the volume keys do; DSlot keeps no level of its own. The slider follows the volume keys (and anything else) while the page is shown, from PipeWire's change events (`nibkit::device::watch_volume`), and steps onto multiples of 5. |
| Top screen, Bottom screen | `/run/nibos/backlight/top` and `bottom` (nibos-hw), written as the slider moves, in steps of 5% that land on multiples of 5 (`nibkit::backlight::step`: 39% goes to 40% or 35%), as Menu + volume steps. 0% is nibOS's floor (2% of the backlight's range on the DS+), never darker; the backlight's own steps already look even ("non-linear" in sysfs), otherwise the percentage is squared. When the slider is let go of, `nibos-backlight save` keeps the levels across an unclean power-off. |
| Wi-Fi | `nib wifi on` and `off` (the Wi-Fi radio alone, by rfkill; on brings the network up first where it isn't), `nib wifi status`. Bluetooth is its own switch, through BlueZ. |
| Battery | Shown, not changed: the charge (the RK817's coulomb count, `nibkit::battery::read`, the same as the status bar's) and what it is doing: charging with an estimate of the time to full (to 80% with protect on), holding at 80% for protect, full. Read every 3 s while the page is shown. |
| Speaker tuning | The speaker amps' tuning (`nibkit::audio`): nibOS's own (the DSP bypassed, conservative) or the official one, which **Import official tuning** brings in from your own stock firmware (a stock card in TF2, or a stock image in Roms; it can take a few minutes, in the background). Both apply at the next restart, which the page then offers (**Restart now**). Hidden where the system has no such choice. |
| Battery protect | `nib battery protect on` / `off` (`nibkit::battery::set_protect`): charging stops at 80% and starts again at 75%, so a device left on its charger ages slower. Followed when changed elsewhere (`nibkit::battery::watch`). |

Menu + volume up / down changes both screens' brightness from anywhere; that is nibOS's own
(nibos-input), not DSlot's. nibOS is the one place the levels are kept: DSlot keeps none, and
reads them when the menu opens, when the System page is turned to, and whenever they change
while it is shown (`nibkit::backlight::watch`: inotify on the levels nibOS's links point at), so
its sliders follow Menu + volume. Volume and Wi-Fi are read when the page is turned to.

## The screens

Mainline KMS gives each panel its own CRTC, and SDL's KMSDRM a display each, so each screen gets
an SDL window and GLES renderer of its own (on the stock OS, Weston shows one display spanning
both). The touch panel is DSI-1, the left-hand display. SDL's atomic KMSDRM path crashes on the
second display, so DSlot sets `SDL_KMSDRM_ATOMIC=0`, which games it starts inherit.

### Handing the screens to a game, and back

DSlot closes its windows before a game starts, so that the game can take the screens, and opens
them again when it ends. Were that all, the text console would show in between: when the last
open handle on the display device closes, the kernel puts the console's framebuffer back on the
panels ("lastclose"), and it stays until the next program takes them. Measured on the DS+ (DRM
debugfs state sampled every 0.4 ms), the console was on screen for ~0.6 s as a game started and
~0.5 s as it ended.

So DSlot opens the card itself at start-up, while no one holds it (`src/kms.rs`), and keeps it
open for its whole life: the last close never happens while DSlot runs. And it keeps a picture
up: the shelf's last frame is read back before the windows close and copied into DSlot's own
KMS buffers (which SDL puts back on screen as it closes, being what it found there), and DSlot
becomes DRM master for a moment to set them, then lets go for the game. When the game ends,
the same frame goes up again until the shelf's first. Measured the same way after: DSlot's
frame, then the game's, then DSlot's, with nothing between, for DSperate (DS and GBA), inkdot
and an app.

Any program on nibOS that hands the screens to another and takes them back should do the same:
keep the card open across the handover, and keep a picture of its own on the outputs.

## Start-up

Each stage is logged against the kernel's clock (CLOCK_BOOTTIME), `dslot: 2.731 s skeleton
shown`, which nibOS's `bench/boot-ui.sh` reads.

1. **A picture at once.** With nibOS's power-on splash (`nibos/rootfs/splash`), the DSlot logo
   is already on the top screen from ~1.5 s after reset, before systemd; DSlot sees
   `/run/nibos/splash` and lets SDL take the screens straight over. Without it, DSlot puts a
   **skeleton** of the shelf (its layout, empty, drawn in software) straight on both CRTCs
   through KMS (`src/kms.rs`) before SDL starts.
2. **Everything slow at the same time.** Mesa's libraries are loaded on a thread from the first
   instruction; each page's games are listed on a thread of their own, and the cart shell drawn
   on another, while SDL, the windows and GL start on the main thread.
3. **Nothing slow before the first frame.** Controllers and audio are opened after it: SDL opens
   every input device to ask what it is, and on nibOS each close waits for an RCU grace period
   (~0.5 s in all); audio is ~0.7 s cold. The touch panel is found by name in sysfs rather than
   by opening every device, and looked for again for half a minute if its driver is still loading.

Once the shelf's first frame is up, the skeleton's buffers go.

## Games

DSlot lets go of both panels (it closes SDL) before a game starts and takes them back when it
exits. Games run inside `dslot.service`, with its limits: real-time priority up to 10, for an
emulator's frame and audio threads. With DSperate, Menu in a game saves and comes back to the
shelf, and starting the game again resumes where it left off.

## Measured on the DS+

Shelf on screen from reset, `nibos/bench/boot-ui.sh`, 3 restarts each, skeleton start (no
power-on splash), 251 DS and 162 GBA games (all zipped) on the TF2 card (17 Oct 2026):

| | Shelf on screen | Mean |
|---|---|---|
| DS page only | 5.27, 5.29, 5.38 s | 5.31 s |
| DS and GBA pages | 5.21, 5.40, 5.25 s | 5.29 s |

DSlot 0.3.0, from its package (19 Oct 2026, same device and games): 5.27, 5.28, 5.32, 5.36,
5.48 s, mean 5.34 s; the first boot after installing it took 5.99 s.

A game's start and return, by DSlot's and DSperate's marks: A → a DS game's first frame 1.97 s
(the insert animation's 1.05 s included); Menu → the shelf again 0.4–0.6 s.

The very first start after adding games reads every new ROM's header: 162 zipped GBA games took
~0.9 s, once. A GBA game from the shelf: A → the game's first frame 1.6–1.8 s (the 1.0 s insert
animation included); Menu → the shelf again 0.43–0.48 s.
