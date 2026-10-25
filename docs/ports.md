# Ports (PortMaster)

DSlot lists PortMaster's ports on a **Ports** page, there when a card has some or PortMaster is
installed.

- **Where:** `Roms/PORTS` on either card. Each port is a script, `<Title>.sh`, and a folder of
  its own, as PortMaster installs them. When the same script is on both cards, the DS card's
  is listed.
- **Name:** `port.json`'s `attr.title`, else `gameinfo.xml`'s `<name>`, else the script's name.
  The folder is the one whose `port.json` lists the script in `items`, or else the folder named
  as the script.
- **Picture:** the one `port.json` names (`attr.image.screenshot`, or a cover), else
  `gameinfo.xml`'s `<image>`, else `screenshot.*` or `cover.*` in the folder. PNG or JPEG.
- **Starting one:** an emulator description for the `PORTS` system, which the `portmaster`
  package installs (`/usr/share/dslot/emulators/portmaster.toml`):

  ```toml
  name = "PortMaster"
  priority = 100
  requires = ["/usr/lib/portmaster/run-port"]
  [systems]
  PORTS = ["/usr/lib/portmaster/run-port", "{rom}"]
  ```

  Ports keep their own saves in their folders, so there is no `{saves}`. A port plays on the top
  screen. While it runs, the bottom screen shows its name and how to leave it: Select + Start or
  Menu + Start quits, and holding power opens the power menu.
- **PortMaster itself** (browsing and installing ports) is the Ports page's first tile, **Get
  more ports**, whatever the page is sorted by, and is on the Apps page too. The package
  installs it in `/usr/share/dslot/apps/portmaster/` (an `app.toml` folder, as in
  `Roms/APPS`); the tile is there whenever it is installed, with no ports yet and no
  `Roms/PORTS` at all. Until PortMaster has fetched itself (its `app.toml`'s `[until]` file,
  `~/.local/share/PortMaster/version`, isn't there yet), the Apps entry and the tile's second
  line read **Get PortMaster**. When it exits, DSlot looks for ports again, so what it installed or
  removed is on the page straight away (DSlot also watches the folders while it runs).
- **Card:** a neutral one (`data/systems/ports.toml`), with no stats or photo, since ports aren't
  a console.
