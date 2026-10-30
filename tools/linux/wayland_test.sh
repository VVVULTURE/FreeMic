#!/bin/bash
# Wayland smoke test: private headless Weston compositor (no X server), FreeMic with the
# null audio backend. Checks that FreeMic starts as a native Wayland client (libwayland +
# EGL loaded, DISPLAY unset), shows its window (screenshot) and quits cleanly.
# Usage: wayland_test.sh [binary] [out-dir]
set -u
BIN=${1:-$HOME/freemic/target/release/freemic}; OUT=${2:-$HOME/wayland-test}
rm -rf "$OUT"; mkdir -p "$OUT"
RT=$(mktemp -d /tmp/fm-wl-XXXX); chmod 700 "$RT"
export XDG_RUNTIME_DIR=$RT FREEMIC_BACKEND=null FREEMIC_DATA_DIR=$OUT/data
unset DISPLAY WAYLAND_DISPLAY PULSE_SERVER
weston --backend=headless --renderer=${RENDERER:-gl} --width=1100 --height=750 --socket=wl-fm --debug \
  --log="$OUT/weston.log" >/dev/null 2>&1 & W=$!
for i in $(seq 1 50); do [ -S "$RT/wl-fm" ] && break; sleep 0.1; done
export WAYLAND_DISPLAY=wl-fm
"$BIN" >"$OUT/freemic.stdout" 2>&1 & A=$!
sleep 4
PASS=0; FAIL=0
check() { if eval "$2"; then echo "PASS: $1"; PASS=$((PASS+1)); else echo "FAIL: $1"; FAIL=$((FAIL+1)); fi; }
check "FreeMic running under Wayland" 'kill -0 $A'
awk '$6 ~ /\.so/ {print $6}' /proc/$A/maps 2>/dev/null | sort -u > "$OUT/libs.txt"
check "uses libwayland-client" 'grep -q libwayland-client "$OUT/libs.txt"'
check "uses EGL" 'grep -q libEGL "$OUT/libs.txt"'
# (libX11 itself may be mapped: Mesa's EGL links it. There is no X server here at all.)
check "DISPLAY unset, so no X11 fallback was possible" '[ -z "${DISPLAY:-}" ]'
(cd "$OUT" && timeout 10 weston-screenshooter >/dev/null 2>&1)
SHOT=$(ls "$OUT"/wayland-screenshot-*.png 2>/dev/null | head -1)
check "screenshot taken" '[ -n "$SHOT" ]'
"$BIN" --quit >/dev/null 2>&1
for i in $(seq 1 25); do kill -0 $A 2>/dev/null || break; sleep 0.2; done
check "hotkeys reported as unavailable on Wayland" 'grep -q "not available in a Wayland session" "$OUT/data/freemic.log"'
check "FreeMic quit cleanly" '! kill -0 $A 2>/dev/null && grep -q "FreeMic exited" "$OUT/data/freemic.log"'
kill $A $W 2>/dev/null
case "$RT" in /tmp/fm-wl-*) rm -rf "$RT";; esac
echo "== wayland: $PASS passed, $FAIL failed (screenshot: ${SHOT:-none})"
