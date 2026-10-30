#!/bin/bash
# List the shared libraries FreeMic has loaded after start-up (including dlopen'ed
# X11/GL/xkbcommon), using a private Xvfb display and the null audio backend.
# Usage: loaded_libs.sh [path-to-freemic]
BIN=${1:-$HOME/freemic/target/release/freemic}
export XDG_RUNTIME_DIR=$(mktemp -d /tmp/fm-libs-XXXX); chmod 700 "$XDG_RUNTIME_DIR"
unset WAYLAND_DISPLAY PULSE_SERVER
export DISPLAY=:78 FREEMIC_BACKEND=null FREEMIC_NO_HOTKEYS=1 FREEMIC_DATA_DIR=$XDG_RUNTIME_DIR/data
Xvfb :78 -screen 0 1024x768x24 -nolisten tcp >/dev/null 2>&1 & X=$!
sleep 1
"$BIN" >/dev/null 2>&1 & APP=$!
sleep 4
if kill -0 $APP 2>/dev/null; then
  awk '$6 ~ /\.so/ {print $6}' /proc/$APP/maps | sort -u
else
  echo "FreeMic is not running (crashed?)"; cat "$FREEMIC_DATA_DIR/freemic.log"
fi
"$BIN" --quit >/dev/null 2>&1; sleep 1; kill $APP $X 2>/dev/null
rm -rf "$XDG_RUNTIME_DIR"
