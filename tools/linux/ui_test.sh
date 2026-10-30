#!/bin/bash
# UI automation test on a private Xvfb display (null audio backend, nothing reaches the
# desktop): reorder buttons by dragging, rename a sound through the right-click menu,
# change the soundboard volume with the slider, restart FreeMic and check that all of
# it persisted.
# Buttons get distinct colors so their positions can be found in screenshots.
# Usage: ui_test.sh [binary]   (needs Xvfb, xdotool, ImageMagick)
set -u
BIN=${1:-$HOME/freemic/target/release/freemic}
FMTEST=$HOME/freemic/target/release/fmtest
OUT=$HOME/ui-test; rm -rf "$OUT"; mkdir -p "$OUT/data"
RT=$(mktemp -d /tmp/fm-ui-XXXX); chmod 700 "$RT"
export XDG_RUNTIME_DIR=$RT DISPLAY=:81 FREEMIC_BACKEND=null FREEMIC_NO_HOTKEYS=1 FREEMIC_DATA_DIR=$OUT/data
unset WAYLAND_DISPLAY
PASS=0; FAIL=0
check() { if eval "$2"; then echo "PASS: $1"; PASS=$((PASS+1)); else echo "FAIL: $1"; FAIL=$((FAIL+1)); fi; }

"$FMTEST" tone "$OUT/a.wav" 5 440 0.2 >/dev/null
"$FMTEST" tone "$OUT/b.wav" 5 550 0.2 >/dev/null
"$FMTEST" tone "$OUT/c.wav" 5 660 0.2 >/dev/null
cat > "$OUT/data/soundboard.json" <<EOF
{"version":1,"next_id":4,"sounds":[
 {"id":1,"name":"Alpha","file":"$OUT/a.wav","mode":"Once","color":[200,40,40],"duration":2.0},
 {"id":2,"name":"Bravo","file":"$OUT/b.wav","mode":"Once","color":[40,160,60],"duration":2.0},
 {"id":3,"name":"Charlie","file":"$OUT/c.wav","mode":"Once","color":[40,80,200],"duration":2.0}]}
EOF

Xvfb :81 -screen 0 1000x700x24 -nolisten tcp >/dev/null 2>&1 & X=$!
sleep 1
# The null backend records FreeMic's mix (the virtual-mic signal) to a WAV file.
start() { FREEMIC_NULL_OUT=$OUT/mix-$1.wav "$BIN" >>"$OUT/freemic.stdout" 2>&1 & APP=$!; sleep 3; xdotool mousemove 990 690; sleep 0.3; }
press() { xdotool mousemove $1 $2; sleep 0.1; xdotool mousedown 1; sleep 0.1; xdotool mouseup 1; }
shot() { import -display :81 -window root "$OUT/$1.png" 2>/dev/null; }
# Bounding box "x y w h" of the pixels of one button color in a screenshot.
box() { convert "$OUT/$1.png" -fuzz 6% -fill black +opaque "rgb($2)" -format "%@" info: 2>/dev/null \
        | sed -E 's/([0-9]+)x([0-9]+)\+([0-9]+)\+([0-9]+)/\3 \4 \1 \2/'; }
center() { set -- $(box "$1" "$2"); [ $# -eq 4 ] && echo "$(( $1 + $3 / 2 )) $(( $2 + $4 / 2 ))"; }
names() { grep -o '"name": *"[^"]*"' "$OUT/data/soundboard.json" | sed -E 's/.*"([^"]*)"$/\1/' | tr '\n' ',' ; }

start 1
shot before
RED=$(center before "200,40,40"); GREEN=$(center before "40,160,60"); BLUE=$(center before "40,80,200")
echo "button centers: Alpha $RED, Bravo $GREEN, Charlie $BLUE"
check "three colored buttons found" '[ -n "$RED" ] && [ -n "$GREEN" ] && [ -n "$BLUE" ]'

# 1. Drag Alpha onto Charlie's place.
set -- $RED; rx=$1; ry=$2; set -- $BLUE; bx=$1; by=$2
xdotool mousemove $rx $ry mousedown 1; sleep 0.2
for i in 1 2 3 4 5 6 7 8; do xdotool mousemove $(( rx + (bx - rx) * i / 8 )) $(( ry + (by - ry) * i / 8 )); sleep 0.05; done
xdotool mouseup 1; sleep 0.3; xdotool mousemove 990 690
sleep 2   # settings are saved ~0.8 s after a change
echo "order after drag: $(names)"
check "drag moved Alpha to the end (saved)" '[ "$(names)" = "Bravo,Charlie,Alpha," ]'

shot afterdrag
# 1b. Click Charlie (660 Hz, 5 s long; now the 2nd button) to play it, click again after ~2 s to stop it.
set -- $(center afterdrag "40,80,200"); press $1 $2; sleep 2; press $1 $2; sleep 1.5; xdotool mousemove 990 690

# 2. Rename Bravo through the context menu ("Rename…" is the first item).
GREEN=$(center afterdrag "40,160,60"); set -- $GREEN; gx=$1; gy=$2
xdotool mousemove $gx $gy click 3; sleep 0.5
shot menu
xdotool mousemove $(( gx + 30 )) $(( gy + 14 )) click 1; sleep 0.5
shot dialog
# No window manager on Xvfb: give the window keyboard focus explicitly.
WID=$(xdotool search --pid $APP 2>/dev/null | tail -1); [ -n "$WID" ] && xdotool windowfocus --sync "$WID"
xdotool key ctrl+a; xdotool type --delay 30 "Renamed Bravo"; xdotool key Return
sleep 2
echo "names after rename: $(names)"
check "rename saved" '[ "$(names)" = "Renamed Bravo,Charlie,Alpha," ]'

# 3. Click near the left end of the top-bar "Sounds" slider (the first teal rail).
shot slider
convert "$OUT/slider.png" -crop 1000x36+0+0 +repage "$OUT/topbar.png"
set -- $(box topbar "0,92,128"); sx=${1:-0}; sy=$(( ${2:-0} + ${4:-0} / 2 ))
echo "Sounds slider rail starts at x=$sx y=$sy"
xdotool mousemove $(( sx + 12 )) $sy; sleep 0.3; xdotool mousedown 1; sleep 0.2; xdotool mousemove $(( sx + 8 )) $sy; sleep 0.2; xdotool mouseup 1; sleep 0.3; shot afterslider; xdotool mousemove 990 690
sleep 2
VOL=$(grep -o '"sound_volume": *[0-9.]*' "$OUT/data/settings.json" | grep -o '[0-9.]*$')
echo "sound_volume after click: $VOL"
check "Sounds slider changed the saved volume" 'awk -v v="$VOL" "BEGIN{exit !(v < 30)}"'

# 4. Restart: the changes must be loaded again.
"$BIN" --quit >/dev/null 2>&1
for i in $(seq 1 25); do kill -0 $APP 2>/dev/null || break; sleep 0.2; done
check "FreeMic quit" '! kill -0 $APP 2>/dev/null'
"$FMTEST" analyze "$OUT/mix-1.wav" 660 440 > "$OUT/clicks.txt"; cat "$OUT/clicks.txt"
ON=$(awk '$1=="t=" && $6+0 > 0.05' "$OUT/clicks.txt" | wc -l)
check "clicking a button plays it into the virtual mic (660 Hz audible $ON s)" '[ "$ON" -ge 1 ]'
check "clicking it again stopped it early (5 s sound, $ON s heard)" '[ "$ON" -le 3 ]'
check "no other sound played" '! awk "\$1==\"t=\" && \$8+0 > 0.01 {f=1} END{exit !f}" "$OUT/clicks.txt"'
start 2
shot restarted
"$BIN" --play "Renamed Bravo" >/dev/null 2>&1; rc=$?
check "renamed sound found after restart (freemic --play \"Renamed Bravo\")" '[ $rc -eq 0 ]'
"$BIN" --play "#3" >/dev/null 2>&1; rc=$?
check "button #3 exists after restart" '[ $rc -eq 0 ]'
set -- $(center restarted "200,40,40"); ax=${1:-0}; set -- $(center restarted "40,160,60"); gx2=${1:-0}
check "Alpha is still drawn after Bravo (order kept)" '[ "$ax" -gt "$gx2" ]'
"$BIN" --quit >/dev/null 2>&1; sleep 1
VOL2=$(grep -o '"sound_volume": *[0-9.]*' "$OUT/data/settings.json" | grep -o '[0-9.]*$')
check "volume kept across the restart ($VOL2)" '[ "$VOL2" = "$VOL" ]'
kill $APP $X 2>/dev/null
case "$RT" in /tmp/fm-ui-*) rm -rf "$RT";; esac
echo "== ui: $PASS passed, $FAIL failed (screenshots in $OUT)"
