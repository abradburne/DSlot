#!/bin/bash
# Start dslot at power-on. Undo with "dslot_boot_off", from dslot's own menu, or by deleting
# update/update.sh from this card on a computer.

card="$(cd "$(dirname "$0")/../.." && pwd)"
hook="$card/update/update.sh"
if [ -f "$hook" ] && ! grep -q dslot "$hook"; then
	echo "dslot: $hook belongs to something else; left alone" > /dev/console
	exit 1
fi
# Where earlier versions put it.
grep -qs dslot /mnt/mmc/dmenu.bin && rm -f /mnt/mmc/dmenu.bin
mkdir -p "$card/update" && cp "$card/Roms/APPS/dslot/boot.sh" "$hook" && sync
