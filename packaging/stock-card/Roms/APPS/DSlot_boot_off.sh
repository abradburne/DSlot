#!/bin/bash
# Go back to the stock menu at power-on.

card="$(cd "$(dirname "$0")/../.." && pwd)"
hook="$card/update/update.sh"
grep -qs dslot /mnt/mmc/dmenu.bin && rm -f /mnt/mmc/dmenu.bin
if grep -qs dslot "$hook"; then
	grep -qs dslot "$card/update/early.sh" && rm -f "$card/update/early.sh"
	rm -f "$hook" && rmdir "$card/update" 2> /dev/null
	sync
fi
