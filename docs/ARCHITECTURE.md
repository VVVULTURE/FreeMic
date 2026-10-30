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
| GUI | egui / eframe 0.37 (OpenGL via glow; X11 and Wayland via winit) | Small immediate-mode UI with no web engine; repaints only when something changes |
| Audio, Windows | WASAPI shared mode through the `wasapi` crate | Event-driven, sample-rate conversion by Windows (`AUTOCONVERTPCM`), device notifications, MMCSS "Pro Audio" thread priority. Shared mode never locks other apps out of a device |
| Audio, Linux | PulseAudio protocol (`libpulse-binding`, asynchronous API) | Can create virtual devices (modules) at run time without root. Works with PulseAudio and with PipeWire through `pipewire-pulse`. The async API is needed for the `DONT_MOVE` stream flag (see below) |
| Decoding | Symphonia 0.6 (pure Rust) | MP3, WAV, OGG Vorbis, FLAC, AAC, ALAC with no FFmpeg or system codecs |
| File resampling | rubato 5 (windowed-sinc) | High quality (tests require > 60 dB SNR) |
| Mic clock-drift correction | own cubic resampler with a PI controller | Needs tiny, continuously varying ratio changes, which a block resampler doesn't do well |
| Thread communication | `rtrb` lock-free ring buffers, atomics | The audio callback never locks or allocates |
| Hotkeys | `global-hotkey` | RegisterHotKey on Windows, XGrabKey on X11 |
| Dialogs | `rfd` | Native file dialogs |

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
The installer and the app can download it from vb-audio.com.
`install-vbcable.ps1` checks the Authenticode signature (must be valid and
from VB-Audio/Burel) before starting VB-Audio's own setup with an elevation
prompt. Silent installs never do this.

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

Device changes come from `IMMNotificationClient` (Windows) or Pulse
subscription events (Linux) and wake the supervisors immediately. A specific
microphone that disappears is waited for, never replaced. "System default"
follows the system default. On Windows the audio threads register with
MMCSS ("Pro Audio"). On Linux they run at normal priority; the sound server
does the time-critical scheduling, and FreeMic's buffers (~30 ms for the
mic, ~340 ms of decoded sound) absorb normal scheduling delays.

## Safety rules built into the code

- FreeMic never changes the system default input or output device.
- Monitoring is off by default and only goes to a device the user picked.
- Linux streams use `DONT_MOVE`, so audio is never re-routed to speakers.
- The virtual microphone is never offered or used as FreeMic's own input.
- One instance per user (mutex `Local\FreeMic-main` / Unix socket). A
  second start shows a message instead of opening the devices twice.

## Persistence

`settings.json` and `soundboard.json` are written atomically (temp file +
rename) shortly after a change and on exit. Unknown fields are ignored and
missing ones get defaults. A damaged file is renamed to
`*.damaged-<timestamp>.json` and the user is told. A UTF-8 BOM, as written
by some editors, is accepted. Imported sounds are copied into `sounds/` as
`<id>-<name>.<ext>`.

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
src/ui.rs                the egui application
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
