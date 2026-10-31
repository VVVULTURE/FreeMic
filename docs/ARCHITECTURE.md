# FreeMic architecture

## Goals that shaped the design

1. Other programs must see a **microphone** that carries the user's voice
   plus the soundboard, mixed live.
2. Never disturb the user's audio setup: don't change default devices,
   don't play to speakers unless asked, don't capture a device whose content
   is FreeMic's own output (feedback).
3. Small and quiet: one native process, near-zero CPU when idle, no
   background services.
4. Same program and behaviour on Windows, Debian/Ubuntu and Arch.

## Technology choices

| Area | Choice | Why |
|---|---|---|
| Language | Rust (edition 2024) | Native speed and a small footprint with no runtime; memory safety on the real-time audio path; one code base for all platforms |
| GUI | egui / eframe 0.37 (OpenGL via glow; X11 and Wayland via winit), with FreeMic's own theme, widgets and painted icons, and the Inter typeface (OFL-1.1, embedded) | Small immediate-mode UI with no web engine; repaints only when something changes |
| Audio, Windows | WASAPI shared mode through the `wasapi` crate | Event-driven, sample-rate conversion by Windows (`AUTOCONVERTPCM`), device notifications, MMCSS "Pro Audio" thread priority. Shared mode never locks other apps out of a device |
| Audio, Linux | PulseAudio protocol (`libpulse-binding`, asynchronous API) | Can create virtual devices (modules) at run time without root. Works with PulseAudio and with PipeWire through `pipewire-pulse`. The async API is needed for the `DONT_MOVE` stream flag (see below) |
| Decoding | Symphonia 0.6 (pure Rust) | MP3, WAV, OGG Vorbis, FLAC, AAC, ALAC with no FFmpeg or system codecs |
| File resampling | rubato 5 (windowed-sinc) | High quality (tests require > 60 dB SNR) |
| Mic clock-drift correction | own cubic resampler with a PI controller | Needs tiny, continuously varying ratio changes, which a block resampler doesn't do well |
| Thread communication | `rtrb` lock-free ring buffers, atomics | The audio callback never locks or allocates |
| Hotkeys | `global-hotkey` | RegisterHotKey on Windows, X11 key grabs; no keyboard hooks |
| Dialogs | `rfd` | Native file dialogs |
| YouTube | yt-dlp (separate program; ships with FreeMic on Windows, downloaded on first use on Linux), `ureq` + rustls for HTTPS, `sha2` | See [YouTube downloads](#youtube-downloads) |

## The virtual microphone

### Windows: VB-CABLE

A program running as a normal user can't create a capture (microphone)
endpoint on Windows. Only a kernel-mode audio driver can, and 64-bit Windows
only loads drivers signed through Microsoft. Options considered:

| Option | Verdict |
|---|---|
| Own driver | Rejected: signing needs an EV certificate and Microsoft attestation, which an open-source project without a company can't get. |
| VirtualDrivers/Virtual-Audio-Driver (MIT) | Rejected: its release is signed by SignPath Foundation, not through Microsoft; its README says beta and "requires test signing"; its microphone is a test signal, not a loopback. |
| **VB-CABLE** (VB-Audio, donationware) | **Chosen**: signed, widely used, a true loopback (anything played into `CABLE Input` comes out of the microphone `CABLE Output`). |

VB-Audio publishes no redistribution permission, so VB-CABLE isn't bundled.
FreeMic also doesn't download or start it: up to 0.2.0 a PowerShell helper
(`-ExecutionPolicy Bypass`) downloaded VB-Audio's zip and started its setup
elevated, which is exactly the pattern of malware droppers. Since 0.2.1 the
app and the installer's last page only open <https://vb-audio.com/Cable/>,
and the user runs VB-Audio's own installer (administrator approval is
needed for any driver). The output supervisor looks for the cable every 3 s,
so FreeMic starts using it as soon as it is installed.

FreeMic finds the cable by name: `CABLE Input` first, then `CABLE-A..D
Input`, `Line N (Virtual Audio Cable)`, and `CABLE In 16ch`. From that it
works out the partner microphone name (`CABLE Output…`), which it shows to
the user and **never captures**. The capture-device list hides it, and the
system-default microphone is skipped if it happens to be that partner, so
FreeMic can't loop its own output back into itself.

### Linux: PulseAudio/PipeWire modules

At start-up FreeMic loads two modules into the user's sound server:

```
module-null-sink    sink_name=freemic_sink  "FreeMic Mixer (internal)"   rate=48000, stereo
module-remap-source source_name=freemic_mic "FreeMic Virtual Microphone" master=freemic_sink.monitor
```

FreeMic plays the mix into `freemic_sink`. The remap source turns the sink's
monitor into a normal source, so programs list it as a microphone rather
than as "Monitor of …", which many apps hide. On exit the modules are
unloaded, in reverse order. A panic hook does the same, and at start-up
leftovers from a crash (matched by name) are removed first. No root rights
and no files on disk are involved.

All FreeMic streams are created with **`DONT_MOVE`**. Without it, if
`freemic_sink` disappeared, the server would move FreeMic's output stream to
the default sink, i.e. the user's speakers. With it the stream fails, and
FreeMic recreates the sink and reconnects (the end-to-end test removes the
sink and checks that nothing ends up on the speakers).

## Audio engine

```
               capture thread                       output thread (clock master)
 microphone ─► (WASAPI / Pulse) ─► mic ring ─►  ┌───────────────────────────────┐
                                    (f32)       │ Mixer::process                 │
                                                │  mic: DriftResampler ─► ×g_mic │
 sound file ─► player thread  ─► block queue ─► │  soundboard blocks ─► ×g_sound │──► ×g_master ─► limiter ─► virtual mic
             (decode + resample)   (64×256 fr)  │  (fade-out on switch)          │
                                                └──────────┬────────────────────┘
                                                           └─► soundboard only ─► monitor ring ─► monitor thread ─► headphones (optional)
```

- **The output device is the clock.** The mixer runs inside the output
  device's callback and pulls everything else. Only the microphone has its
  own clock, so only the microphone needs drift correction.
- **Player thread.** Decodes the selected file a little ahead into blocks of
  256 frames, at most 64 blocks (~340 ms at 48 kHz), resampling to the
  output rate with rubato. Looping restarts the decoder at the end without
  a gap.
- **Single-sound semantics with generations.** Every play/stop increments
  `target_gen`. The mixer drops any block whose generation is older than the
  target, so a switch takes effect within one output period. The start of
  the old sound's remaining audio is faded out over 128 frames (~2.7 ms) to
  avoid a click. A new sound starts only once the player has queued its
  first 4 blocks (`ready_gen`), so it can't run dry inside its first period.
- **Gains** are `(percent/100)²` and ramped linearly across each period (no
  zipper noise). The mic slider goes to 200 %.
- **Limiter**: look-ahead peak limiter (1.5 ms look-ahead, ceiling −1 dBFS)
  on the sum. It only acts when mic + soundboard would clip.
- **Monitor** gets a copy of the soundboard only, scaled by the soundboard
  and monitor volumes, never the microphone (no self-echo, no feedback).

### Microphone drift correction

The mic and output devices run on different crystals, typically tens to
hundreds of ppm apart. Without correction the mic ring would slowly fill up
(growing delay) or run empty (dropouts). `DriftResampler`:

1. **Prebuffer** until the ring holds the goal level (target latency plus
   one output period, ~30 ms at 48 kHz/10 ms periods), then fade in.
2. **Settle** for 0.3 s at nominal rate while measuring the average fill.
   Capture and output periods are rarely in phase. A big surplus or deficit
   is corrected once, by skipping or holding a few ms hidden under a 1 ms
   duck, instead of by a slow pitch slew.
3. **Run**: a PI controller on the smoothed fill adjusts the resampling
   ratio by at most ±0.4 %, inaudible, typically a few hundred ppm.
4. On an underrun it fades out and prebuffers again. On a huge backlog
   (e.g. after a system stall) it jumps back to the goal.

`FREEMIC_STATS=1` logs fill, goal, ratio and underruns twice a second.

## Threads and failure handling

| Thread | Role |
|---|---|
| UI (main) | egui window, settings, saves JSON |
| output | opens the virtual-mic feed device, runs the mixer |
| capture | opens the physical microphone, fills the mic ring |
| monitor | optional, plays the monitor ring |
| player | decodes files |
| supervisors | one per stream: (re)open the device, retry every 3 s on failure, restart on device change notifications |
| IPC | named pipe / Unix socket server for `--play/--stop/--quit` |
| youtube | runs one YouTube job at a time (set up / check / update yt-dlp, search, look up, download) and starts yt-dlp as a child process |

Device changes come from `IMMNotificationClient` (Windows) or Pulse
subscription events (Linux) and wake the supervisors immediately. A specific
microphone that disappears is waited for, never replaced. "System default"
follows the system default. On Windows the audio threads register with
MMCSS ("Pro Audio"). On Linux they run at normal priority; the sound server
does the time-critical scheduling, and FreeMic's buffers (~30 ms for the
mic, ~340 ms of decoded sound) absorb normal scheduling delays.

## User interface

`src/ui/` holds the egui application: `mod.rs` (pages, sound pad grid,
dialogs, settings), `theme.rs` (dark and light palettes, Inter at two
weights from one variable font file, widget styling), `widgets.rs` (buttons,
tabs, cards, sliders, settings rows, text that is elided to a width),
`icons.rs` (vector icons drawn with lines and shapes) and `youtube_view.rs`.

Layouts are computed from the available width instead of fixed positions:
the header drops its wordmark and then its labels when space runs out, the
bottom bar switches to two rows below 700 px, settings rows put the label
above the control below 470 px, sound names wrap to at most three lines and
are elided with "…", and every page scrolls. The minimum window size is
480 × 400. `tests/ui_layout.rs` renders every page headless at nine window
sizes from 480 × 400 to 1920 × 1080, at 100 % and 150 % scale, in both
themes and in many states (playing, missing virtual mic, dialogs, YouTube
results, errors, downloads), and fails if any two visible texts overlap, if
any text is cut off at the side or leaves the window, if clickable controls
overlap, or if text half-covers a control.

## YouTube downloads

The **YouTube** page ports the search and download logic of
[YT-Downloader](https://github.com/VVVULTURE/YT-Downloader) (Python) to Rust
(`src/youtube.rs`), keeping yt-dlp as the downloader as YT-Downloader does:

- **yt-dlp on Windows ships with FreeMic.** `packaging/windows/build.ps1`
  fetches the official `yt-dlp.exe` of a release, checks it against that
  release's `SHA2-256SUMS`, and puts it next to `FreeMic.exe` with its
  licenses (`yt-dlp-LICENSE.txt`, `yt-dlp-THIRD_PARTY_LICENSES.txt`,
  `yt-dlp-SOURCE.txt`). FreeMic.exe never downloads, writes or starts program
  files of its own: FreeMic 0.2.0 downloaded yt-dlp.exe into its data folder
  and Microsoft Defender flagged FreeMic (`Trojan:Win32/Bearfoos.B!ml`, a
  machine-learning detection) within 0.1 s of it saving that file. The
  version is read from yt-dlp.exe's version resource (no process is started
  for it). **Update yt-dlp** runs yt-dlp's own updater (`yt-dlp -U`), which
  downloads the newest release, checks it against `SHA2-256SUMS` and replaces
  itself; FreeMic then deletes the `yt-dlp.exe.old` it leaves behind. The
  installer doesn't overwrite a newer yt-dlp.exe on upgrade.
- **yt-dlp on Linux** is downloaded on first use: FreeMic finds the newest
  release from where `github.com/yt-dlp/yt-dlp/releases/latest` redirects
  (as YT-Downloader does), downloads `yt-dlp_linux` (x86-64) and that
  release's `SHA2-256SUMS` over HTTPS, verifies the SHA-256 and only then
  moves it into `<data>/tools/`. The standalone build needs no Python; it was
  tested on Debian 12 (glibc 2.36), Debian 13 and Arch.
- **Updates**: FreeMic asks GitHub for the newest version at most every 6
  hours (YT-Downloader's interval), only while the YouTube page or its
  Settings card is used, and offers **Update yt-dlp** when there is a newer
  one. It never installs an update by itself. If GitHub can't be reached,
  the installed copy is used.
- **Search**: `ytsearch10:<query>` with `--flat-playlist`. Live streams and
  premieres are left out. Music search builds the query and ranks results
  exactly like YT-Downloader's `score_music`; Python's
  `difflib.SequenceMatcher.ratio` was ported and checked against Python on
  3,000 generated cases (`difflib_cases_from_python`, an ignored test).
- **Never a playlist**: pasted links are parsed by FreeMic itself (watch,
  youtu.be, shorts, embed, live, music.youtube.com, nocookie). A link with
  `list=` is reduced to its video id; playlist, channel and other pages are
  refused before yt-dlp runs. yt-dlp is always given
  `https://www.youtube.com/watch?v=<id>` with `--no-playlist
  --max-downloads 1 --ignore-config`, and FreeMic accepts the result only if
  exactly one file was produced. (Tested: `--no-playlist` alone does not stop
  a `/playlist?list=` link, which is why links are reduced first.)
- **Format**: `bestaudio[ext=m4a]/bestaudio[acodec^=mp4a]`, i.e. YouTube's
  AAC stream, which Symphonia decodes, so no ffmpeg is needed. Downloads go to
  `sounds/youtube/.downloading/` and are moved to `sounds/youtube/<id>.m4a`
  when complete.
- **Reliability**: the player clients are tried in YT-Downloader's order
  (`default,-tv`, `default,web_embedded`, `web_safari,mweb`, `ios`,
  `android`), stopping early for private/members-only/removed videos and
  missing formats, with `--retries 5 --fragment-retries 10`. Error messages
  follow YT-Downloader (bot check, private/removed, unavailable). If Deno,
  Node.js or Bun is on the `PATH`, it is passed to yt-dlp with
  `--js-runtimes`; without one, the audio downloads still worked in testing.
- **Processes**: on Windows yt-dlp is put in a job object
  (`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`) right after it starts, so **Cancel**
  ends it and the second process it unpacks into (`TerminateJobObject`), and
  nothing of it can outlive the job, even if FreeMic itself crashes. On
  Linux it runs in its own process group, which Cancel ends with `SIGTERM`.
  On Windows yt-dlp runs without a console window (`CREATE_NO_WINDOW`).

## Safety rules built into the code

- FreeMic never changes the system default input or output device.
- Monitoring is off by default and only goes to a device the user picked.
- Linux streams use `DONT_MOVE`, so audio is never re-routed to speakers.
- The virtual microphone is never offered or used as FreeMic's own input.
- One instance per user (mutex `Local\FreeMic-main` / Unix socket). A
  second start shows a message instead of opening the devices twice.
- Network access happens only while the user uses YouTube features
  (GitHub for yt-dlp versions and, on Linux, its download; YouTube through
  yt-dlp). No telemetry.
- FreeMic.exe never downloads, writes or starts program files, never runs
  PowerShell or scripts, and declares `asInvoker` in its manifest (it never
  asks for administrator rights). The only programs it starts are yt-dlp and,
  when the user clicks a link or "Open folder", the browser / file manager.
- yt-dlp is verified against its published SHA-256 before it is used and
  runs with `--ignore-config`.

## Persistence

`settings.json` and `soundboard.json` are written atomically (temp file +
rename) shortly after a change and on exit. Unknown fields are ignored and
missing ones get defaults. A damaged file is renamed to
`*.damaged-<timestamp>.json` and the user is told. A UTF-8 BOM, as written
by some editors, is accepted. Imported sounds are copied into `sounds/` as
`<id>-<name>.<ext>`, YouTube downloads are `sounds/youtube/<video id>.m4a`,
and `tools/yt-dlp.json` holds the last update check (plus, on Linux, the
downloaded yt-dlp and its version).

## Performance

The UI repaints only while a sound plays, while Settings shows live meters,
or after input. Otherwise the process sleeps in the audio callbacks.
Measured (10 s of process CPU time, 20 logical cores):

| | idle (mic passthrough on) | playing a looped sound | memory |
|---|---|---|---|
| Windows 11, VB-CABLE | 0.02–0.08 % | 0.06 % | working set ~93 MB |
| Debian 13, PulseAudio (Xvfb, software GL) | 0.04 % | 0.34 % | RSS ~117 MB |
| Arch, PipeWire (Xvfb, software GL) | 0.03 % | 0.11 % | RSS ~154 MB |

Most of the memory is the OpenGL driver: Mesa's LLVM-based software
renderer in the Linux tests, and the GPU driver's user-mode part on
Windows.

## Source layout

```
src/main.rs              start-up, command line, single instance, IPC commands
src/ui/                  the egui application (mod.rs, theme.rs, widgets.rs, icons.rs, youtube_view.rs)
src/youtube.rs           yt-dlp setup, link parsing, search/ranking, downloads
assets/fonts/            Inter variable font (embedded) and its license
src/audio/engine.rs      threads, supervisors, device selection, public engine API
src/audio/mixer.rs       real-time mix
src/audio/player.rs      decoder thread
src/audio/decode.rs      Symphonia + rubato streaming decoder
src/audio/drift.rs       mic drift-correcting resampler
src/audio/limiter.rs     look-ahead limiter
src/audio/backend/       windows.rs (WASAPI), linux.rs (PulseAudio), null.rs (tests)
src/config.rs            settings and soundboard files
src/library.rs           importing/copying sounds
src/hotkeys.rs           global hotkeys
src/ipc.rs, platform.rs, paths.rs, logging.rs, icon.rs
src/bin/fmtest.rs        test tool: list devices/sessions, record, analyze, tones, icons
```
