#!/bin/bash
# Installed as update/update.sh on this card by "Start at power-on". The stock boot script
# runs that file before its own menu comes up, so nothing on the system partitions is changed.
# Leaving dslot (Select + Start) carries on to the stock menu.

card="$(cd "$(dirname "$0")/.." && pwd)"
if [ -f "$card/Roms/APPS/dslot/dslot" ]; then
	echo "$(date '+%F %T') started at power-on, $(cut -d' ' -f1 /proc/uptime) s after the kernel" >> "$card/Roms/APPS/dslot/boot.log"
	bash "$card/Roms/APPS/dslot.sh"
fi
exit 0
