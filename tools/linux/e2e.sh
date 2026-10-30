#!/bin/bash
# Isolated end-to-end test of FreeMic on Linux (run as an unprivileged user).
#
# Starts a PRIVATE sound server (PulseAudio or PipeWire) in a private
# XDG_RUNTIME_DIR with only virtual devices, and a private Xvfb display, so
# nothing reaches real hardware, the desktop, or WSLg. Then:
#   - feeds a 220 Hz "microphone" through a fake mic source,
#   - runs FreeMic (native backend) against that server,
#   - records FreeMic's virtual microphone like a receiving app would,
#   - plays/loops/replaces/stops sounds over the command socket,
#   - checks the recording, device-disconnect recovery, server restart,
#     and that FreeMic removes its virtual devices on exit.
#
# Usage: e2e.sh pulseaudio|pipewire
set -u
SERVER=${1:-pulseaudio}
BIN=~/freemic/target/release
# FREEMIC=/usr/bin/freemic tests an installed package instead of the build tree.
APPBIN=${FREEMIC:-$BIN/freemic}
OUT=~/e2e-$SERVER
rm -rf "$OUT"; mkdir -p "$OUT"
export XDG_RUNTIME_DIR=$(mktemp -d /tmp/fm-rt-XXXX); chmod 700 "$XDG_RUNTIME_DIR"
unset PULSE_SERVER WAYLAND_DISPLAY PIPEWIRE_REMOTE
export DISPLAY=:77
export FREEMIC_DATA_DIR=$OUT/data
printf 'autospawn = no
' > "$OUT/client.conf"; export PULSE_CLIENTCONFIG=$OUT/client.conf
PASS=0; FAIL=0
ok()   { echo "PASS: $*"; PASS=$((PASS+1)); }
bad()  { echo "FAIL: $*"; FAIL=$((FAIL+1)); }
check(){ if eval "$2"; then ok "$1"; else bad "$1"; fi; }

# A previous run's private servers may still be shutting down; starting while they
# exit made pipewire-pulse fail once with "File exists". Wait for them first.
for i in $(seq 1 50); do pgrep -u "$(id -u)" -x 'pipewire|pipewire-pulse|wireplumber|pulseaudio' >/dev/null || break; sleep 0.2; done

PIDS=()
cleanup_all() {
  "$APPBIN" --quit >/dev/null 2>&1
  sleep 1
  for p in "${PIDS[@]}"; do kill "$p" 2>/dev/null; done
  pkill -u "$(id -u)" -x freemic 2>/dev/null
  rm -rf "$XDG_RUNTIME_DIR"
}
trap cleanup_all EXIT

Xvfb :77 -screen 0 1100x750x24 -nolisten tcp >/dev/null 2>&1 & PIDS+=($!)

start_server() {
  if [ "$SERVER" = pulseaudio ]; then
    cat > "$OUT/server.pa" <<'EOF'
load-module module-native-protocol-unix
load-module module-null-sink sink_name=fake_speakers sink_properties=device.description=FakeSpeakers
load-module module-null-sink sink_name=fakemic_feed sink_properties=device.description=FakeMicFeed
load-module module-remap-source master=fakemic_feed.monitor source_name=fake_mic source_properties=device.description=FakeMicrophone
set-default-sink fake_speakers
set-default-source fake_mic
EOF
    pulseaudio -n -F "$OUT/server.pa" --daemonize=no --exit-idle-time=-1 --disallow-exit --log-target=file:"$OUT/pulseaudio.log" & SRV=$!
  else
    if [ -z "${DBUS_SESSION_BUS_ADDRESS:-}" ]; then
      export DBUS_SESSION_BUS_ADDRESS=unix:path=$XDG_RUNTIME_DIR/bus
      dbus-daemon --session --address="$DBUS_SESSION_BUS_ADDRESS" --nofork --nopidfile >/dev/null 2>&1 & PIDS+=($!)
      sleep 0.5
    fi
    pipewire >>"$OUT/pipewire.log" 2>&1 & PIDS+=($!); PW=$!
    sleep 0.5
    wireplumber >>"$OUT/wireplumber.log" 2>&1 & PIDS+=($!); WP=$!
    for try in 1 2 3; do
      pipewire-pulse >>"$OUT/pipewire-pulse.log" 2>&1 & SRV=$!
      for i in $(seq 1 50); do pactl info >/dev/null 2>&1 && break; kill -0 $SRV 2>/dev/null || break; sleep 0.2; done
      pactl info >/dev/null 2>&1 && break
      echo "pipewire-pulse did not start (try $try), retrying"; kill $SRV 2>/dev/null; sleep 1
    done
    pactl load-module module-null-sink sink_name=fake_speakers sink_properties=device.description=FakeSpeakers >/dev/null
    pactl load-module module-null-sink sink_name=fakemic_feed sink_properties=device.description=FakeMicFeed >/dev/null
    pactl load-module module-remap-source master=fakemic_feed.monitor source_name=fake_mic source_properties=device.description=FakeMicrophone >/dev/null
    sleep 1
    pactl set-default-sink fake_speakers; pactl set-default-source fake_mic
  fi
  PIDS+=($SRV)
  for i in $(seq 1 50); do pactl info >/dev/null 2>&1 && break; sleep 0.2; done
}
start_server
pactl info | grep -E "Server Name|Default Sink|Default Source"
check "private server reachable and isolated" '[ "$(pactl info | grep -c "Default Sink: fake_speakers")" = 1 ]'

# Test material: 220 Hz mic tone (looped forever), 440 Hz 2 s sound, 1 kHz loop sound.
"$BIN/fmtest" tone "$OUT/mic220.wav" 300 220 0.3 >/dev/null
"$BIN/fmtest" tone "$OUT/beep440.wav" 2 440 0.5 >/dev/null
"$BIN/fmtest" tone "$OUT/loop1k.wav" 1 1000 0.5 >/dev/null
mic_feed() { while true; do paplay --device=fakemic_feed "$OUT/mic220.wav" || sleep 0.5; done; }
mic_feed & MICFEED=$!; PIDS+=($MICFEED)

mkdir -p "$FREEMIC_DATA_DIR"
cat > "$FREEMIC_DATA_DIR/soundboard.json" <<EOF
{"version":1,"next_id":3,"sounds":[
 {"id":1,"name":"Beep","file":"$OUT/beep440.wav","mode":"Once","duration":2.0},
 {"id":2,"name":"Hum","file":"$OUT/loop1k.wav","mode":"Loop","duration":1.0,"hotkey":"control+shift+F9"}]}
EOF
echo '{"sound_volume":100,"mic_volume":100,"master_volume":100,"stop_hotkey":"control+shift+F10"}' > "$FREEMIC_DATA_DIR/settings.json"
DEF_SRC_BEFORE=$(pactl get-default-source)

FREEMIC_STATS=1 "$APPBIN" >"$OUT/freemic.stdout" 2>&1 & APP=$!; PIDS+=($APP)
for i in $(seq 1 50); do pactl list short sources | grep -q freemic_mic && break; sleep 0.2; done
sleep 1
pactl list sources | grep -E "Name: freemic_mic|Description: FreeMic" > "$OUT/sources.txt"
check "virtual microphone source exists" 'grep -q "Name: freemic_mic" "$OUT/sources.txt"'
check "virtual microphone has a friendly name" 'grep -q "Description: FreeMic Virtual Microphone" "$OUT/sources.txt"'
check "FreeMic did not change the default source" '[ "$(pactl get-default-source)" = "$DEF_SRC_BEFORE" ]'
check "FreeMic did not change the default sink" '[ "$(pactl get-default-sink)" = fake_speakers ]'
pactl list source-outputs | grep -E "application.name|Source:" > "$OUT/capture-streams.txt"
check "FreeMic captures the (fake) physical mic" 'grep -q "FreeMic" "$OUT/capture-streams.txt"'

# Underrun counters once startup is over (a launch-time hiccup is recovered with a fade
# and reported separately).
BASE_MIC=$(grep "stats: mic" "$FREEMIC_DATA_DIR/freemic.log" | tail -1 | sed -E 's/.*mic_underruns ([0-9]+).*/\1/'); BASE_MIC=${BASE_MIC:-0}
echo "mic underruns during startup: $BASE_MIC"

# Receiving app: record the virtual microphone for 12 s while driving the soundboard.
timeout 16 parecord --device=freemic_mic --rate=48000 --channels=2 --format=float32le --file-format=wav "$OUT/received.wav" & REC=$!
# Start the timeline only once the recorder is really capturing (~50 ms of data on disk).
for i in $(seq 1 100); do [ "$(stat -c %s "$OUT/received.wav" 2>/dev/null || echo 0)" -gt 20000 ] && break; sleep 0.05; done
sleep 1.95; "$APPBIN" --play Beep
sleep 4;  "$APPBIN" --play Hum
sleep 2;  "$APPBIN" --play Beep
sleep 2;  "$APPBIN" --stop
wait $REC 2>/dev/null
"$BIN/fmtest" analyze "$OUT/received.wav" 220 440 1000 | tee "$OUT/analysis.txt"
# Format of analysis lines: "  t=  3s rms 0.3  220Hz 0.300  440Hz 0.500  1000Hz 0.000"
line() { grep -E "t= *$1s " "$OUT/analysis.txt"; }
val() { line "$1" | sed -E "s/.* $2Hz ([0-9.]+).*/\1/"; }
near() { awk -v a="$1" -v b="$2" -v tol="$3" 'BEGIN{exit !((a-b)<tol && (b-a)<tol)}'; }
# Anchor on the first second where the Beep is audible (commands are 2 s apart).
T0=$(awk '{for(i=1;i<=NF;i++) if($i=="440Hz" && $(i+1)>0.25){sub("t=","",$1); gsub("s","",$1); print ($1==""?$2:$1); exit}}' "$OUT/analysis.txt" | tr -dc 0-9)
T0=${T0:-99}; echo "Beep first audible in second $T0"
at() { echo $((T0 + $1)); }
# (0.03: the onset may fall a few ms before a second boundary; a playing Beep reads 0.5)
check "before Beep: mic only (220 present)" 'near "$(val $(at -1) 220)" 0.30 0.04 && near "$(val $(at -1) 440)" 0 0.03'
check "mic + Beep mixed"                  'near "$(val $(at 1) 220)" 0.30 0.04 && near "$(val $(at 1) 440)" 0.50 0.06'
check "Beep ended by itself"              'near "$(val $(at 3) 440)" 0 0.01 && near "$(val $(at 3) 220)" 0.30 0.04'
check "loop playing with mic"             'near "$(val $(at 5) 1000)" 0.50 0.06 && near "$(val $(at 5) 220)" 0.30 0.04'
check "Beep replaced loop"                'near "$(val $(at 7) 1000)" 0 0.01 && near "$(val $(at 7) 440)" 0.50 0.06'
check "stopped, mic continues"            'near "$(val $(at 9) 440)" 0 0.01 && near "$(val $(at 9) 1000)" 0 0.01 && near "$(val $(at 9) 220)" 0.30 0.04'
check "no clipping in received audio"     'grep -q "clipped_samples 0" "$OUT/analysis.txt"'
LAST=$(grep "stats: mic" "$FREEMIC_DATA_DIR/freemic.log" | tail -1); echo "last stats: $LAST"
check "no soundboard or mic underruns"    'echo "$LAST" | grep -q "mic_underruns $BASE_MIC sound_underruns 0" && ! grep -q "underrun #" "$FREEMIC_DATA_DIR/freemic.log"'

# Processing load: CPU time of the FreeMic process over 10 s, idle and while playing.
load() { # $1 = label
  local t0 t1 hz=$(getconf CLK_TCK) n=$(nproc)
  t0=$(awk '{print $14+$15}' /proc/$APP/stat); sleep 10; t1=$(awk '{print $14+$15}' /proc/$APP/stat)
  awk -v a="$t0" -v b="$t1" -v hz="$hz" -v n="$n" -v l="$1" -v rss="$(awk '/VmRSS/ {print $2}' /proc/$APP/status)" \
    'BEGIN{printf "%s: CPU %.3f%% of all cores (%d logical), RSS %.1f MB\n", l, (b-a)/hz/10/n*100, n, rss/1024}'
}
{ load "idle (mic passthrough active)"; "$APPBIN" --play Hum >/dev/null; load "playing a looped sound + mic"; "$APPBIN" --stop >/dev/null; } | tee "$OUT/perf.txt"

# Global hotkeys (X11): keys are injected into the private Xvfb display only.
timeout 12 parecord --device=freemic_mic --rate=48000 --channels=2 --format=float32le --file-format=wav "$OUT/hotkeys.wav" & REC=$!
for i in $(seq 1 100); do [ "$(stat -c %s "$OUT/hotkeys.wav" 2>/dev/null || echo 0)" -gt 20000 ] && break; sleep 0.05; done
sleep 1; xdotool key --clearmodifiers ctrl+shift+F9    # Hum (loop)
sleep 3; xdotool key --clearmodifiers ctrl+shift+F10   # stop all
wait $REC 2>/dev/null
"$BIN/fmtest" analyze "$OUT/hotkeys.wav" 1000 > "$OUT/hotkeys.txt"; cat "$OUT/hotkeys.txt"
hk() { grep -E "t= *$1s " "$OUT/hotkeys.txt" | sed -E "s/.* 1000Hz ([0-9.]+).*/\1/"; }
# Anchor on the first second where the loop is fully audible (server latency varies).
H0=$(awk '$1 == "t=" && $NF+0 > 0.45 {sub(/s$/, "", $2); print $2; exit}' "$OUT/hotkeys.txt"); H0=${H0:-99}
echo "hotkey loop fully audible from second $H0"
check "hotkey starts a sound (Ctrl+Shift+F9)" '[ "$H0" -le 4 ] && near "$(hk $H0)" 0.50 0.06'
check "stop hotkey stops it (Ctrl+Shift+F10)" '[ -n "$(hk $((H0 + 4)))" ] && near "$(hk $((H0 + 4)))" 0 0.01'

# Screenshot of the running UI (private Xvfb display).
import -display :77 -window root "$OUT/ui.png" 2>/dev/null && ok "screenshot taken" || bad "screenshot"

# Microphone disconnect and reconnect.
MICMOD=$(pactl list short modules | awk '/source_name=fake_mic/ {print $1}')
pactl unload-module "$MICMOD"
sleep 2
timeout 3 parecord --device=freemic_mic --rate=48000 --channels=2 --format=float32le --file-format=wav "$OUT/nomic.wav"
"$BIN/fmtest" analyze "$OUT/nomic.wav" 220 > "$OUT/nomic.txt"
check "mic unplugged: no mic signal, no crash" 'kill -0 $APP && near "$(grep -E "t= *1s " "$OUT/nomic.txt" | sed -E "s/.* 220Hz ([0-9.]+).*/\1/")" 0 0.01'
# FreeMic's capture streams may only ever be attached to the real (fake) mic.
fm_clients() { pactl list short clients | awk '/FreeMic/ {print $1}'; }
wrong_streams() { # $1 = sink-inputs|source-outputs, $2 = the only allowed device index
  local c; for c in $(fm_clients); do pactl list short "$1" | awk -v c="$c" -v ok="$2" '$3==c && $2!=ok {print}'; done; }
idx_of() { pactl list short "$1" | awk -v n="$2" '$2==n {print $1}'; }
check "mic unplugged: no other source substituted" '[ -z "$(wrong_streams source-outputs NONE)" ]'
pactl load-module module-remap-source master=fakemic_feed.monitor source_name=fake_mic source_properties=device.description=FakeMicrophone >/dev/null
pactl set-default-source fake_mic
sleep 5
timeout 3 parecord --device=freemic_mic --rate=48000 --channels=2 --format=float32le --file-format=wav "$OUT/remic.wav"
"$BIN/fmtest" analyze "$OUT/remic.wav" 220 > "$OUT/remic.txt"
check "mic reconnected: passthrough recovers" 'near "$(grep -E "t= *1s " "$OUT/remic.txt" | sed -E "s/.* 220Hz ([0-9.]+).*/\1/")" 0.30 0.05'

# Internal sink removed under FreeMic: its stream must NOT be moved to the speakers.
SINKMOD=$(pactl list short modules | awk '/sink_name=freemic_sink/ {print $1}')
SRCMOD=$(pactl list short modules | awk '/source_name=freemic_mic/ {print $1}')
pactl unload-module "$SRCMOD"; pactl unload-module "$SINKMOD"
sleep 0.5
BAD=""; for i in 1 2 3 4 5 6; do BAD="$BAD$(wrong_streams sink-inputs "$(idx_of sinks freemic_sink)")"; sleep 0.5; done
echo "streams on wrong sinks: ${BAD:-none}"
check "mix never re-routed to real speakers" '[ -z "$BAD" ]'
sleep 5
check "virtual microphone re-created after removal" 'pactl list short sources | grep -q freemic_mic'

# Sound server restart (e.g. user restarts PipeWire): FreeMic must come back.
if [ "$SERVER" = pipewire ]; then kill "$SRV" "$WP" "$PW"; else kill "$SRV"; fi
sleep 1
start_server
sleep 6
check "virtual microphone back after server restart" 'pactl list short sources | grep -q freemic_mic'
check "FreeMic still running after server restart" 'kill -0 $APP'

# Graceful exit removes everything FreeMic created.
"$APPBIN" --quit
for i in $(seq 1 30); do kill -0 $APP 2>/dev/null || break; sleep 0.2; done
check "FreeMic exited" '! kill -0 $APP 2>/dev/null'
check "virtual devices removed on exit" '! pactl list short modules | grep -q freemic'
check "settings were saved" '[ -s "$FREEMIC_DATA_DIR/settings.json" ]'
check "log written" 'grep -q "created virtual microphone" "$FREEMIC_DATA_DIR/freemic.log"'
echo "== $SERVER: $PASS passed, $FAIL failed"
[ $FAIL -eq 0 ]
