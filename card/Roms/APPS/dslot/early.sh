#!/bin/bash
# Installed as update/early.sh by "Start at power-on", on the card the OS starts from. The
# stock system never runs this file. dsboot, if it is installed, runs it as soon as the screen
# is up, about a second before the stock boot script gets as far as update.sh beside it; that
# script then finds dslot already started and waits for it.

card="$(cd "$(dirname "$0")/.." && pwd)"
[ -f "$card/Roms/APPS/dslot/dslot" ] || exit 0
echo $$ > /tmp/dslot.early

# What the stock launch script sets for the things it starts.
export SDL_VIDEO_DOUBLE_BUFFER=1
export SDL_RENDER_VSYNC=1

# The stock launch script, which has only just started, is what mounts the games card. If
# there is an SD card in the other slot, dslot waits for it before it looks for games.
this=$(awk -v at="$card" '$2 == at { print $1 }' /proc/mounts | tail -n 1)
for disk in /sys/block/mmcblk[0-9]; do
	case "$this" in "/dev/${disk##*/}"*) continue ;; esac
	if [ "$(cat "$disk/device/type" 2> /dev/null)" = SD ]; then
		export DSLOT_WAIT_FOR=/mnt/sdcard
	fi
done

echo "$(date '+%F %T') started early, $(cut -d' ' -f1 /proc/uptime) s after the kernel" >> "$card/Roms/APPS/dslot/boot.log"
bash "$card/Roms/APPS/dslot.sh"
echo $? > /tmp/dslot.early.done
