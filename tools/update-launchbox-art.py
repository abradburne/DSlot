#!/usr/bin/env python3
"""Build compact zlib-compressed LaunchBox artwork indexes in data/art/<key>.tsv.z.

Reads LaunchBox's daily public dump (https://gamesdb.launchbox-app.com/Metadata.zip) and extracts
only game names, alternate names, regions, and image filenames for the consoles DSlot supports.
No artwork binaries are stored in the repository; DSlot downloads individual images on demand
from https://images.launchbox-app.com/<FileName>.

Usage:
    python3 tools/update-launchbox-art.py [path/to/Metadata.zip]
"""
from __future__ import annotations

import collections
import datetime
import os
import sys
import tempfile
import urllib.request
import xml.etree.ElementTree as ET
import zipfile
import zlib

METADATA_URL = "https://gamesdb.launchbox-app.com/Metadata.zip"

# LaunchBox platform name -> (DSlot console key, cart/disc media type(s))
PLATFORMS: dict[str, tuple[str, tuple[str, ...]]] = {
    "Nintendo DS": ("ds", ("Cart - Front",)),
    "Nintendo Game Boy Advance": ("gba", ("Cart - Front",)),
    "Nintendo Game Boy": ("gb", ("Cart - Front",)),
    "Nintendo Game Boy Color": ("gbc", ("Cart - Front",)),
    "Super Nintendo Entertainment System": ("snes", ("Cart - Front",)),
    "Nintendo Entertainment System": ("nes", ("Cart - Front",)),
    "Nintendo Famicom Disk System": ("fds", ("Cart - Front", "Disc")),
    "Sega Genesis": ("md", ("Cart - Front",)),
    "Sega CD": ("mcd", ("Disc",)),
    "Sega 32X": ("32x", ("Cart - Front",)),
    "Sega Pico": ("pico", ("Cart - Front",)),
    "Sega Master System": ("sms", ("Cart - Front",)),
    "Sega SG-1000": ("sg", ("Cart - Front",)),
    "Sega Game Gear": ("gg", ("Cart - Front",)),
    "NEC TurboGrafx-16": ("pce", ("Cart - Front",)),
    "PC Engine SuperGrafx": ("pce", ("Cart - Front",)),
    "NEC TurboGrafx-CD": ("pcecd", ("Disc",)),
    "Sony Playstation": ("ps", ("Disc",)),
    "Sony PSP": ("psp", ("Disc",)),
    "Atari Lynx": ("lynx", ("Cart - Front",)),
    "SNK Neo Geo Pocket": ("ngp", ("Cart - Front",)),
    "SNK Neo Geo Pocket Color": ("ngp", ("Cart - Front",)),
    "WonderSwan": ("ws", ("Cart - Front",)),
    "WonderSwan Color": ("ws", ("Cart - Front",)),
    "Nintendo Game & Watch": ("gw", ("Cart - Front",)),
    "Arcade": ("arcade", ()),
}


def clean(text: str | None) -> str:
    return " ".join((text or "").replace("\t", " ").split())


def norm_region(region: str) -> str:
    r = region.strip().lower()
    if not r:
        return ""
    if any(k in r for k in ("north america", "united states", "usa", "canada")):
        return "US"
    if "world" in r:
        return "W"
    if any(k in r for k in ("europe", "united kingdom", "germany", "france", "spain", "italy", "australia", "netherlands", "sweden")):
        return "EU"
    if "japan" in r:
        return "JP"
    if "korea" in r:
        return "KR"
    if "asia" in r or "china" in r or "taiwan" in r:
        return "AS"
    return ""


def pick_per_region(items: list[tuple[str, str]]) -> str:
    """Keep one image per canonical region (preferring PNG over JPEG), in stable region order."""
    by_reg: dict[str, str] = {}
    for raw_reg, fn in items:
        reg = norm_region(raw_reg)
        prev = by_reg.get(reg)
        if prev is None or (prev.lower().endswith(".jpg") and fn.lower().endswith(".png")):
            by_reg[reg] = fn
    order = {"US": 0, "W": 1, "EU": 2, "JP": 3, "": 4, "KR": 5, "AS": 6}
    pairs = sorted(by_reg.items(), key=lambda kv: (order.get(kv[0], 9), kv[0]))
    return ";".join(f"{reg}={fn}" if reg else fn for reg, fn in pairs)


def build(archive_path: str, out_dir: str) -> None:
    games: dict[str, tuple[str, str, tuple[str, ...]]] = {}  # db_id -> (key, name, cart_types)
    alts: dict[str, set[str]] = collections.defaultdict(set)
    carts: dict[str, list[tuple[str, str]]] = collections.defaultdict(list)
    boxes: dict[str, list[tuple[str, str]]] = collections.defaultdict(list)

    with zipfile.ZipFile(archive_path) as zf:
        dated = datetime.date(*zf.getinfo("Metadata.xml").date_time[:3]).isoformat()
        with zf.open("Metadata.xml") as metadata:
            for _, el in ET.iterparse(metadata):
                tag = el.tag
                if tag == "Game":
                    plat = el.findtext("Platform") or ""
                    spec = PLATFORMS.get(plat)
                    if spec is not None:
                        gid = el.findtext("DatabaseID") or ""
                        name = clean(el.findtext("Name"))
                        if gid and name:
                            games[gid] = (spec[0], name, spec[1])
                    el.clear()
                elif tag == "GameAlternateName":
                    gid = el.findtext("DatabaseID") or ""
                    if gid in games:
                        alt = clean(el.findtext("AlternateName"))
                        if alt and "|" not in alt:
                            alts[gid].add(alt)
                    el.clear()
                elif tag == "GameImage":
                    gid = el.findtext("DatabaseID") or ""
                    if gid in games:
                        img_type = el.findtext("Type") or ""
                        fn = (el.findtext("FileName") or "").strip()
                        if fn and not img_type.startswith("Fanart"):
                            reg = el.findtext("Region") or ""
                            if img_type in games[gid][2]:
                                carts[gid].append((reg, fn))
                            elif img_type == "Box - Front":
                                boxes[gid].append((reg, fn))
                    el.clear()

    # Group by DSlot console key; merge duplicate titles within combined consoles (e.g. NGP+NGPC).
    by_key: dict[str, list[str]] = collections.defaultdict(list)
    for gid, (key, name, _) in games.items():
        c = pick_per_region(carts[gid]) if gid in carts else ""
        b = pick_per_region(boxes[gid]) if gid in boxes else ""
        if not c and not b:
            continue
        other = "|".join(sorted(a for a in alts[gid] if a != name))
        by_key[key].append(f"{name}\t{other}\t{c}\t{b}")

    os.makedirs(out_dir, exist_ok=True)
    total_z = 0
    total_games = 0
    for key in sorted(by_key):
        lines = sorted(set(by_key[key]))
        raw = ("\n".join([f"# {dated} launchbox"] + lines) + "\n").encode("utf-8")
        compressed = zlib.compress(raw, 9)
        path = os.path.join(out_dir, f"{key}.tsv.z")
        with open(path, "wb") as fh:
            fh.write(compressed)
        total_z += len(compressed)
        total_games += len(lines)
        print(f"  {key:8s} {len(lines):5d} games  {len(raw)//1024:4d} KB -> {len(compressed)//1024:4d} KB")
    print(f"total: {total_games} games in {len(by_key)} files, {total_z/1024:.0f} KB compressed")


def main() -> None:
    repo = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    out_dir = os.path.join(repo, "data", "art")
    if len(sys.argv) > 1:
        build(sys.argv[1], out_dir)
    else:
        with tempfile.TemporaryDirectory() as tmp:
            zip_path = os.path.join(tmp, "Metadata.zip")
            print(f"Downloading {METADATA_URL}...")
            req = urllib.request.Request(METADATA_URL, headers={"User-Agent": "DSlot-index-builder/1.0"})
            with urllib.request.urlopen(req, timeout=180) as r, open(zip_path, "wb") as fh:
                while chunk := r.read(1 << 20):
                    fh.write(chunk)
            build(zip_path, out_dir)


if __name__ == "__main__":
    main()
