#!/bin/bash
# DSlot: run from Apps on the stock RG DS+ menu.

progdir="$(cd "$(dirname "$0")" && pwd)/dslot"
up() { cut -d' ' -f1 /proc/uptime; }
t_script=$(up)

. /etc/profile.d/myenv.sh 2>/dev/null
export XDG_RUNTIME_DIR=/var/run
export WAYLAND_DISPLAY=wayland-0
export SDL_VIDEODRIVER=wayland
export WESTON_OUTPUT_FLOW=horizontal

# The stock helper that points sound at the speaker or headphones, sets the saved volume and
# answers the volume, brightness and power keys. It only stays up while the "an app is
# running" flag is set. The stock menu sets that flag when it launches something; when dslot
# starts at power-on the menu has not run, so set it here, as the menu would.
echo 1 > /sys/class/anbernic_misc/runapp
if ! ps -A | grep -q "portsCtrl.dge"; then
	if [ -f /mnt/mmc/portsCtrl.dge ]; then
		/mnt/mmc/portsCtrl.dge > /dev/null 2>&1 &
	else
		/mnt/vendor/bin/portsCtrl.dge > /dev/null 2>&1 &
	fi
fi

# 0 turns the touch panel on.
echo 0 > /sys/class/anbernic_misc/tpctrl

cd "$progdir" || exit 1

# What decides whether dslot starts at power-on, written out for troubleshooting. Left until
# the shelf is up: it took half a second that the shelf was waiting behind.
(
	sleep 5
	date
	echo "== boot_device: $(cat /sys/class/anbernic_misc/boot_device 2>&1)"
	echo "== version"; cat /mnt/vendor/oem/version.ini 2>&1
	echo "== hook"; ls -la /mnt/mmc/dmenu.bin /mnt/vendor/res2.ini /mnt/mod/ctrl/autostart /mnt/sdcard/update /mnt/mmc/update 2>&1
	echo "== mounts"; grep -E " /mnt/| /userdata " /proc/mounts
	echo "== processes"; ps -ef 2>/dev/null | grep -E "loadapp|dmenu|S60|weston|portsCtrl|pulse" | grep -v grep
	echo "== sound"; echo "runapp: $(cat /sys/class/anbernic_misc/runapp 2>&1)"; pactl info 2>&1 | grep -E "Default Sink|Server Name"; pactl list short sinks 2>&1
	echo "== dmenu_ln"; cat /mnt/vendor/ctrl/dmenu_ln 2>&1
	echo "== loadapp tail"; tail -n 25 /mnt/vendor/ctrl/loadapp.sh 2>&1
) > "$progdir/diag.txt" 2>&1 &
{
	echo "DSlot.sh: $t_script s started"
	./dslot "$@"
} > "$progdir/log.txt" 2>&1
