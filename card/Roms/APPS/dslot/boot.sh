#!/bin/bash
# Installed as update/update.sh on this card by "Start at power-on". The stock boot script
# runs that file before its own menu comes up, so nothing on the system partitions is changed.
# Leaving dslot (Select + Start) carries on to the stock menu.

card="$(cd "$(dirname "$0")/.." && pwd)"
# With dsboot installed, early.sh beside this file has started dslot already. Stay here until
# dslot is left, as if this script had started it. If it did not come up, carry on and start it.
if [ -f /tmp/dslot.early ]; then
	early=$(cat /tmp/dslot.early)
	while kill -0 "$early" 2> /dev/null; do
		sleep 0.2
	done
	[ "$(cat /tmp/dslot.early.done 2> /dev/null)" != 0 ] || exit 0
fi
if [ -f "$card/Roms/APPS/dslot/dslot" ]; then
	echo "$(date '+%F %T') started at power-on, $(cut -d' ' -f1 /proc/uptime) s after the kernel" >> "$card/Roms/APPS/dslot/boot.log"
	bash "$card/Roms/APPS/dslot.sh"
fi
exit 0
