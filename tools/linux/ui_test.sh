#!/bin/bash
# UI automation test on a private Xvfb display (null audio backend, nothing reaches the
# desktop): reorder buttons by dragging, rename a sound through the right-click menu,
# change the soundboard volume with the slider, restart FreeMic and check that all of
# it persisted. With --youtube it also uses the YouTube page through the window
# (needs the internet): set up yt-dlp, refuse a playlist link, add one video from a
# link that is inside a mix, and play it.
# Buttons get distinct colors so their positions can be found in screenshots.
# Usage: ui_test.sh [binary] [--youtube]   (needs Xvfb, xdotool, ImageMagick)
set -u
BIN=$HOME/freemic/target/release/freemic; YT=0
for a in "$@"; do case "$a" in --youtube) YT=1;; *) BIN=$a;; esac; done
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
# Dark theme (the accent color below is the dark theme's), microphone passthrough off.
echo '{"version":1,"theme":"Dark","mic_enabled":false,"window_size":[1000,700]}' > "$OUT/data/settings.json"
ACCENT="240,122,74"

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
# Separate areas of one color: "x y w h" per line (bigger than 150 px), left to right.
blobs() { convert "$OUT/$1.png" -fuzz 6% -fill black +opaque "rgb($2)" -fill white -opaque "rgb($2)" \
          -define connected-components:verbose=true -define connected-components:area-threshold=150 \
          -connected-components 8 null: 2>/dev/null | awk '$NF ~ /gray\(255\)|srgb\(255,255,255\)|white/ {print $2}' \
          | sed -E 's/([0-9]+)x([0-9]+)\+([0-9]+)\+([0-9]+)/\3 \4 \1 \2/' | sort -n; }
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

# 3. Click near the left end of the "Sounds" volume slider in the bottom bar (its accent-colored fill).
shot slider
convert "$OUT/slider.png" -crop 1000x90+0+610 +repage "$OUT/dock.png"
set -- $(blobs dock "$ACCENT" | head -1); sx=${1:-0}; sy=$(( 610 + ${2:-0} + ${4:-0} / 2 ))
echo "Sounds slider fill starts at x=$sx y=$sy"
xdotool mousemove $(( sx + 8 )) $sy; sleep 0.3; xdotool mousedown 1; sleep 0.2; xdotool mousemove $(( sx + 6 )) $sy; sleep 0.2; xdotool mouseup 1; sleep 0.3; shot afterslider; xdotool mousemove 990 690
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

if [ $YT -eq 1 ]; then
  # 5. YouTube page, driven through the window. (Volume back to 100 % after the slider test.)
  sed -i -E 's/"sound_volume": *[0-9.]+/"sound_volume": 100.0/' "$OUT/data/settings.json"
  FREEMIC_TEST_VIEW=youtube FREEMIC_NULL_OUT=$OUT/mix-yt.wav "$BIN" >>"$OUT/freemic.stdout" 2>&1 & APP=$!; sleep 3; xdotool mousemove 990 690; sleep 0.3
  WID=$(xdotool search --pid $APP 2>/dev/null | tail -1)
  shot yt-setup
  # The only accent-colored control in the page body is "Set up YouTube downloads".
  convert "$OUT/yt-setup.png" -crop 1000x500+0+110 +repage "$OUT/yt-body.png"
  set -- $(blobs yt-body "$ACCENT" | tail -1); press $(( ${1:-0} + ${3:-0} / 2 )) $(( 110 + ${2:-0} + ${4:-0} / 2 )); xdotool mousemove 990 690
  for i in $(seq 1 90); do [ -f "$OUT/data/tools/yt-dlp.json" ] && break; sleep 1; done
  sleep 1
  check "yt-dlp set up from the page ($(cat "$OUT/data/tools/yt-dlp.json" 2>/dev/null | tr -d '\n '))" '[ -x "$OUT/data/tools/yt-dlp_linux" ] && [ -f "$OUT/data/tools/yt-dlp.json" ]'
  shot yt-form
  convert "$OUT/yt-form.png" -crop 1000x500+0+110 +repage "$OUT/yt-body.png"
  # Accent blobs: the selected mode button (left) and the Search button (right, same row as the field).
  set -- $(blobs yt-body "$ACCENT" | tail -1); SBX=${1:-0}; SBY=$(( 110 + ${2:-0} + ${4:-0} / 2 ))
  echo "Search button at x=$SBX y=$SBY"
  type_query() { xdotool mousemove $(( SBX - 150 )) $SBY click 1; sleep 0.3; [ -n "$WID" ] && xdotool windowfocus --sync "$WID"; xdotool key ctrl+a; xdotool type --delay 15 "$1"; xdotool key Return; xdotool mousemove 990 690; }
  type_query "https://www.youtube.com/playlist?list=PLFgquLnL59alCl_2TQvOiD5Vgm1hCaGSI"; sleep 1.5
  shot yt-playlist
  ERRPIX=$(convert "$OUT/yt-playlist.png" -fuzz 2% -fill black +opaque "rgb(47,31,33)" -format "%[fx:mean*w*h]" info: 2>/dev/null)
  check "playlist link refused with an error message (error box pixels: ${ERRPIX%.*})" 'awk -v v="$ERRPIX" "BEGIN{exit !(v > 500)}"'
  check "nothing downloaded for the playlist link" '[ ! -d "$OUT/data/sounds/youtube" ] || [ -z "$(ls -A "$OUT/data/sounds/youtube")" ]'
  type_query "https://www.youtube.com/watch?v=jNQXAC9IVRw&list=RDjNQXAC9IVRw&start_radio=1"
  ADD=""
  for i in $(seq 1 30); do
    sleep 2; shot yt-result
    convert "$OUT/yt-result.png" -crop 1000x400+0+$(( SBY + 40 )) +repage "$OUT/yt-res.png"
    ADD=$(blobs yt-res "$ACCENT" | tail -1); [ -n "$ADD" ] && break
  done
  check "pasted link looked up: one result with an Add button" '[ -n "$ADD" ]'
  set -- $ADD; press $(( ${1:-0} + ${3:-0} / 2 )) $(( SBY + 40 + ${2:-0} + ${4:-0} / 2 )); xdotool mousemove 990 690
  for i in $(seq 1 60); do grep -q jNQXAC9IVRw "$OUT/data/soundboard.json" 2>/dev/null && break; sleep 1; done
  sleep 1; shot yt-added
  check "video added to the soundboard as \"Me at the zoo\"" 'grep -q "\"name\": *\"Me at the zoo\"" "$OUT/data/soundboard.json"'
  check "stored in the library as sounds/youtube/jNQXAC9IVRw.m4a" 'grep -q "sounds/youtube/jNQXAC9IVRw.m4a" "$OUT/data/soundboard.json" && [ -s "$OUT/data/sounds/youtube/jNQXAC9IVRw.m4a" ]'
  check "exactly one file downloaded (only the video, not the mix)" '[ "$(ls -A "$OUT/data/sounds/youtube" | wc -l)" -eq 1 ]'
  check "no yt-dlp process left running" '! pgrep -x yt-dlp_linux >/dev/null'
  "$BIN" --play "Me at the zoo" >/dev/null 2>&1; rc=$?; sleep 4
  "$BIN" --quit >/dev/null 2>&1
  for i in $(seq 1 25); do kill -0 $APP 2>/dev/null || break; sleep 0.2; done
  "$FMTEST" analyze "$OUT/mix-yt.wav" > "$OUT/yt-play.txt"
  LOUD=$(awk '$1=="t=" && $4+0 > 0.01' "$OUT/yt-play.txt" | wc -l)
  check "the downloaded sound plays into the virtual mic ($LOUD s audible, --play rc=$rc)" '[ $rc -eq 0 ] && [ "$LOUD" -ge 2 ]'
fi
kill $APP $X 2>/dev/null
case "$RT" in /tmp/fm-ui-*) rm -rf "$RT";; esac
echo "== ui: $PASS passed, $FAIL failed (screenshots in $OUT)"
