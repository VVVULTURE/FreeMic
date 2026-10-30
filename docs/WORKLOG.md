# FreeMic work log

Read this first after a context reset. Newest entries at the bottom.

## 2026-10-29 — session 1

**Decisions (with evidence)**
- Language/GUI: Rust + eframe/egui 0.37 (glow). Audio: WASAPI via `wasapi` 0.26 (Windows), libpulse (Linux). Decoding: symphonia 0.6 (MP3/WAV/OGG Vorbis/FLAC/AAC/ALAC). Resampling: rubato 5 (sinc) for files; own cubic drift resampler (PI on ring fill) for mic.
- Windows virtual mic: no user-mode API to create capture endpoints. VirtualDrivers/Virtual-Audio-Driver (MIT) signed release 25.7.14 is signed by **SignPath Foundation (GlobalSign OV)**, not Microsoft; README says "beta, requires test signing"; its mic is a fake mic, not a loopback cable -> rejected. Own driver impossible to sign (needs EV cert + MS attestation). => Use VB-CABLE (VB-Audio, donationware, no redistribution terms published -> not bundled; app links to official page). Auto-detect "CABLE Input"; any cable selectable.
- Linux: plan = libpulse null-sink + remap-source (works on PulseAudio and pipewire-pulse), created at start, removed at exit.

**Machine facts (Vaden's PC)**: VB-CABLE already installed (CABLE Input/Output 48 kHz 2ch). Discord uses "Microphone (Logitech G432)". At 19:40 no app had a session on CABLE Output; "Listen to this device" off on CABLE Output. WSL test distros installed: FreeMic-Arch, FreeMic-Debian (location %LOCALAPPDATA%\FreeMic-TestEnvs; remove with `wsl --unregister`).

**Testing rules**: never render to real speakers; GUI test windows off-screen + non-activating; user mic capture and VB-CABLE end-to-end need care (check sessions with `fmtest list` first). Env overrides: FREEMIC_BACKEND=null, FREEMIC_TEST_MIC=null, FREEMIC_TEST_OUTPUT=null, FREEMIC_NULL_OUT=wav, FREEMIC_NULL_MIC=wav, FREEMIC_DATA_DIR=dir.

**State**: core + UI compile; 35 tests pass (unit: limiter, drift ±3000 ppm, config, library; formats: 18 incl. bad files; pipeline: 6). Fixtures from tools/make_fixtures.sh (ffmpeg sine amplitude is 1/8 -> volume=4).

**Next**: off-screen GUI screenshot (null audio), Linux backend + WSL tests, Windows E2E via VB-CABLE, packaging (Inno Setup, portable zip, deb, PKGBUILD, AppImage), docs, perf measurements.

## 2026-10-29 — session 1, later
- Linux backend: libpulse async streams with DONT_MOVE (simple API cannot set it; without it a vanished freemic_sink would let the server move the mix to the real speakers). Virtual mic = module-null-sink freemic_sink + module-remap-source freemic_mic, created at start, removed on exit, leftovers removed at next start. Panic hook also cleans up.
- Found: X11 needs libxkbcommon-x11.so at runtime (Debian libxkbcommon-x11-0, Arch libxkbcommon-x11) -> package dependency.
- tools/linux/e2e.sh (private server + Xvfb, fake mic, parecord as receiving app): 25/25 on Debian 13 + PulseAudio 17, 25/25 on Arch + PipeWire 1.6.10 (each run twice).
- Windows: named-pipe IPC (`freemic --play/--stop/--quit`). tools/windows/e2e.ps1: 12/12 through VB-CABLE with synthetic mic (aborts if any app has a session on CABLE Output). Idle CPU with passthrough 0.016-0.047% of all cores; WS 93 MB, private 117 MB (GL driver dominated).
- LS-fit glitch scan of received audio: amplitude 0.2497-0.2502 for 0.25 input, median residual -58 dB, no discontinuities; remaining residual = drift controller pitch wander ~0.02 Hz (inaudible).
- VB-CABLE helper packaging/windows/install-vbcable.ps1 -VerifyOnly: downloads official zip, Authenticode OK (BUREL VINCENT).

## 2026-10-29 — session 2 (after context reset)
- **Drift resampler redesign**: FREEMIC_STATS=1 showed the mic buffer starting ~1 period below goal (capture/output phase), so the PI controller pitch-slewed for seconds after start (220 Hz read low on VB-CABLE). Now: Prebuffering -> Settling (0.3 s at nominal rate, measures mean fill) -> one-shot skip (surplus) or hold (deficit) hidden by a 1 ms duck -> Running with gentle PI (max ±0.4 %). Test `startup_surplus_does_not_cause_pitch_slew`.
- **Config BOM**: Windows PowerShell 5.1 `Set-Content -Encoding utf8` writes a BOM; settings were treated as damaged. load_json strips a UTF-8 BOM (test).
- **IPC**: pipe client retries 50x20 ms (server re-creates the pipe between clients).
- **Soundboard gap at sound start (real bug)**: mixer started a sound as soon as its first 256-frame block was queued; with 480-frame periods it could run dry inside the first period (~5 ms hole). Fix: `Shared.ready_gen`; player marks a generation ready after START_BLOCKS=4 blocks (or its end); mixer waits for it. Second bug found by the new underrun log (`stats: soundboard underrun #n: target/ready/audible gens`): replacing a loop whose queue is full of stale blocks hit the "queue full => ready" shortcut before any new block existed -> removed. Tests: mixer `new_sound_waits_until_enough_is_queued`, pipeline `replacement_is_not_ready_until_its_own_blocks_are_queued`; pipeline rig asserts 0 sound underruns every period.
- E2E scripts now assert no soundboard underruns and no mic underruns after startup (baseline taken after launch; one run showed 1 mic underrun inside the first 0.5 s with the synthetic mic, recovered by fade, before any receiver was recording).
- Linux e2e: `pkill -f freemic` killed the caller's shell (`cd ~/freemic`) -> `pkill -x`. PipeWire harness once failed to start pipewire-pulse ("File exists") when runs were back-to-back -> waits for previous servers + retries. Onset-second tolerance 0.01 -> 0.03 (Beep onset a few ms before a second boundary read 0.010).
- **Results now**: cargo test 15 unit + 18 formats + 7 pipeline pass on Windows, Debian, Arch. Windows VB-CABLE e2e 13/13 (x2, earlier 12/12 x4), Debian+PulseAudio 26/26, Arch+PipeWire 26/26 (x5 incl. reruns). Idle CPU 0.023-0.078 % of all cores, WS ~93 MB.

## 2026-10-29 — session 2, packaging
- Windows: rebuilt installer + portable. Installer: silent per-user install (exit 0, exe hash = build), E2E 13/13 against installed exe, default data %APPDATA%\FreeMic, silent uninstall removes dir/uninstall key/Start menu (keeps %APPDATA%\FreeMic in silent mode by design; test data deleted by hand). Portable zip: data in .\data, nothing in %APPDATA%/HKCU, E2E 13/13.
- Linux artifacts built by `tools/linux/build_in_chroot.sh` inside a **Debian 12 bookworm debootstrap chroot** (in FreeMic-Debian at /srv/bookworm, glibc 2.36, rustup 1.99.1): binary needs GLIBC <= 2.35 (hypot). Built on trixie it carried a weak `GLIBC_2.39` (pidfd_spawnp from std) -> loader warning on older systems.
- Verified: portable tar in bookworm chroot 26/26 (PulseAudio 16); .deb on trixie install -> e2e 26/26 -> remove (nothing left), desktop-file-validate OK, lintian only no-manual-page + initial-upload-closes-no-bugs; PKGBUILD on Arch: makepkg incl. tests, namcap (only dlopen "may not be needed" + missing url), pacman -U -> e2e 26/26 -> pacman -R clean; portable on trixie + Arch 26/26.
- Runtime-loaded libs (tools/linux/loaded_libs.sh, /proc/pid/maps): libpulse, X11/X11-xcb, Xcursor, Xi, xkbcommon(+x11), GLVND libGL/GLX (Mesa); on Wayland libwayland-client + EGL.
- Wayland: tools/linux/wayland_test.sh with headless Weston 15 (GL renderer), DISPLAY unset: FreeMic runs as Wayland client, window rendered (screenshot), quits cleanly. Global hotkeys on Wayland not available (documented: bind `freemic --play` in compositor).
- X11 global hotkeys tested in e2e via xdotool into private Xvfb: Ctrl+Shift+F9 starts loop, Ctrl+Shift+F10 (stop hotkey) stops it. Linux e2e now 28 checks, 28/28 on both distros. Windows hotkeys NOT tested (would inject keys into the user's real session).
- Load (10 s CPU time): Windows idle 0.023-0.078 %, playing 0.055 % of 20 logical cores, WS ~93 MB; Debian idle 0.04 %, playing 0.34 %, RSS 117 MB; Arch idle 0.03 %, playing 0.11 %, RSS 154 MB (Linux numbers include llvmpipe software GL on Xvfb).
- **Traps**: `wsl.exe -d X -- cmd` runs through the login shell, so `$VARS` expand there (empty) -> use `wsl.exe -e bash script.sh`. `mount --rbind /dev` + `umount -lR` without `--make-rslave` propagated and broke the test distro's /dev (fixed by `wsl --terminate FreeMic-Debian`). The Bash tool turns `\1` in heredocs passed to python into chr(1): write backslashes via chr(92) or the Edit tool.

## 2026-10-29 — session 2, final state
- Docs written: README, docs/INSTALL, USER-GUIDE, ARCHITECTURE, TROUBLESHOOTING, BUILDING, TESTING, CHECKLIST (status per brief item), man page packaging/linux/freemic.1; screenshots docs/images (Windows off-screen capture, demo board, mic passthrough off).
- Fixes found while documenting/testing: silent installs could start the VB-CABLE download+UAC on PCs without a cable (`Check: not WizardSilent`; tested with a /DTestNoCable build + stub script); `--play N` used creation order -> now button position, `#N` accepted (`ui::find_target`, unit test); GUI-subsystem exe printed nothing for --version/--help and panicked on closed pipe -> AttachConsole(ATTACH_PARENT_PROCESS) + non-panicking writes; Wayland sessions now report hotkeys unavailable (XWayland grab would only work with X11 focus); tools/linux/build.sh aborted under POSIX sh when ~/.cargo/env missing.
- New test scripts: tools/windows/test_installer.ps1 (15), test_portable.ps1 (9), capture_test.ps1 (6; real WASAPI capture of CABLE Output, fmtest play -> CABLE Input, mix to null WAV; 5-min soak 299/300 s at level, drift mean 5 ppm, -480..+575 ppm), tools/linux/ui_test.sh (12; xdotool+ImageMagick on Xvfb: drag reorder, click play/stop, rename via menu, slider, restart persistence), wayland_test.sh (7), test_deb.sh, test_pkgbuild.sh, portable_test.sh, chroot_test.sh, chroot_deb_test.sh, loaded_libs.sh, smoke_paths.sh.
- Final verified artifacts (dist/): Windows setup 15/15 + e2e 13/13, portable 9/9 + e2e 13/13; Linux built in bookworm chroot: .deb Debian 12+13 e2e 28/28, PKGBUILD Arch 28/28, portable Debian 12/13/Arch 28/28, Wayland 7/7, UI 12/12 both distros, cargo test 41/41 Windows/Debian/Arch.
- Not tested (needs Vaden's permission, listed in docs/CHECKLIST.md): physical G432 mic, live Discord call, Windows global hotkeys, Windows device unplug, interactive installer + VB-CABLE driver install, autostart toggle.
- Nothing committed (git init only). WSL test distros FreeMic-Debian (+ /srv/bookworm chroot) and FreeMic-Arch still registered (%LOCALAPPDATA%\FreeMic-TestEnvs).
