#!/bin/bash
# Start FreeMic (null audio, private Xvfb) without overrides, quit it over the
# command socket and show where it stored its data. Usage: smoke_paths.sh [binary]
BIN=${1:-freemic}
RT=$(mktemp -d /tmp/fm-x-XXXX); chmod 700 "$RT"
export XDG_RUNTIME_DIR=$RT DISPLAY=:79 FREEMIC_BACKEND=null FREEMIC_NO_HOTKEYS=1
unset WAYLAND_DISPLAY
Xvfb :79 -nolisten tcp >/dev/null 2>&1 & X=$!
sleep 1
"$BIN" >/dev/null 2>&1 & A=$!
sleep 3
"$BIN" --version
"$BIN" --quit
for i in $(seq 1 25); do kill -0 $A 2>/dev/null || break; sleep 0.2; done
kill -0 $A 2>/dev/null && { echo "still running"; kill $A; } || echo "exited"
kill $X 2>/dev/null
case "$RT" in /tmp/fm-x-*) rm -rf "$RT";; esac
