# Completion checklist

Status values, as requested in the project brief: **Implemented** ·
**Successfully tested** (with evidence) · **Partially tested** ·
**Not implemented** · **Blocked by an external requirement**. Evidence
refers to sections of [TESTING.md](TESTING.md). Items 1–29 are from the
original brief (0.1.0, re-verified with 0.2.0); items 30–37 are from the
second request (0.2.0: visual redesign and YouTube).

| # | Item | Status | Evidence / notes |
|---|---|---|---|
| 1 | Windows 11 fully implemented and tested | **Partially tested** | Everything is implemented. Tested on Windows 11 Pro through VB-CABLE: mixing, play once/loop/replace/stop (§2, 13/13), real WASAPI capture of a virtual capture device incl. a 5-min soak (§3), installer (§4, 15/15), portable (§4, 9/9); all re-run with 0.2.0. Not tested on Windows: your physical microphone, a live Discord call, global hotkeys, device unplugging (all need your permission or would touch your session; §9). |
| 2 | Arch Linux implemented and tested | **Successfully tested** | Arch + PipeWire 1.6.10: e2e 28/28 (dev build, installed package, portable), UI test 12/12 (0.1.0) and 21/21 incl. YouTube (0.2.0), Wayland 7/7, PKGBUILD/makepkg/namcap/install/remove (§5–7). In a clean WSL test distribution, not on bare metal. |
| 3 | Debian implemented and tested | **Successfully tested** | Debian 13 + PulseAudio 17 and Debian 12 + PulseAudio 16 (chroot): e2e 28/28 with the `.deb` and the portable archive; UI test 12/12 (0.1.0) and 21/21 incl. YouTube (0.2.0) (§5–7). Clean WSL test distribution / chroot. |
| 4 | Audio files can be imported | **Successfully tested** | Library import/copy/replace/remove unit test; e2e and UI tests load boards. Import through the file dialog/drag-and-drop UI isn't automated (the dialog is native). |
| 5 | MP3 playback | **Successfully tested** | formats: `mp3_44k_stereo`, `mp3_22k_mono`, `long_file_streams_full_length` (20 s MP3); pipeline test plays an MP3. |
| 6 | WAV playback | **Successfully tested** | formats: 8/16/24-bit, 32-bit float, 8–96 kHz, mono/stereo/5.1; all e2e sounds are WAV. |
| 7 | OGG playback | **Successfully tested** (Vorbis) | `ogg_vorbis_48k_stereo`. Ogg **Opus is not supported** (reported cleanly; test `unsupported_codec_is_reported`). |
| 8 | FLAC playback | **Successfully tested** | `flac_24bit_44k_stereo`, `flac_16k_mono`. |
| 9 | M4A playback | **Successfully tested** | `m4a_aac_44k_stereo`, `m4a_alac_48k_stereo`. |
| 10 | Soundboard buttons work | **Successfully tested** | UI test: a real mouse click plays exactly that sound into the mix, and a second click stops it; `--play` (same code path) in all e2e runs. |
| 11 | Button names and arrangement customizable | **Successfully tested** | UI test: drag-to-reorder and rename via the context menu, persisted across a restart. Colors, hotkeys, "Move left/right", "Replace audio file" are implemented; their UI isn't automated. |
| 12 | Play Once | **Successfully tested** | pipeline `play_once_plays_whole_file_then_stops`; e2e "Beep ended by itself" (Windows + Linux). |
| 13 | Loop | **Successfully tested** | pipeline `loop_repeats_seamlessly_until_stopped`; e2e "loop playing with mic". |
| 14 | New sound stops the previous one | **Successfully tested** | pipeline `new_sound_replaces_old_one_immediately` (old sound inaudible one period after the switch, no glitch); e2e "Beep replaced loop"; no gap at the start of the new sound (§1, §2). |
| 15 | Soundboard volume independent | **Successfully tested** | pipeline `mic_and_sound_are_mixed_with_independent_gains`; UI test saves and restores the Sounds slider. |
| 16 | Physical microphone volume independent | **Successfully tested** (with test signals) | Same pipeline test (mic and sound gains changed separately). Measured with synthetic/fake/virtual microphones, not your physical one. |
| 17 | Mic + soundboard mixed correctly | **Successfully tested** | e2e "mic + Beep mixed": both tones at their exact expected levels in what the receiving app records, on Windows (VB-CABLE) and Linux. |
| 18 | Virtual microphone available to other apps | **Successfully tested** | Windows: VB-CABLE `CABLE Output` recorded in WASAPI shared mode by `fmtest record` like any app. Linux: `FreeMic Virtual Microphone` is a normal source, recorded by `parecord`. |
| 19 | Receiving apps hear the mixed audio | **Partially tested** | Verified with a receiving test application on both platforms (a measured signal, not just device presence). A real Discord call wasn't done (would inject audio into an app in use); **needs your permission**. |
| 20 | No persistent feedback or clipping | **Successfully tested** | `clipped_samples 0` in every e2e recording; limiter tests; `loud_mic_plus_loud_sound_never_clip`. Feedback: the virtual mic is never offered or captured as input (code + Linux e2e "no other source substituted"); the monitor carries the soundboard only. |
| 21 | Device disconnections handled | **Partially tested** | Linux e2e: mic unplugged (no crash, no substitute), mic reconnected (recovers), internal sink removed (no re-routing to speakers, re-created), sound server restarted (recovers). Windows: same supervisor logic, but not tested; unplugging needs physical action or a system change. |
| 22 | Settings persist between launches | **Successfully tested** | UI test: rename, order and volume survive a restart. e2e: stored volumes are applied at launch (measured levels match them). Damaged/BOM files are handled (unit tests). |
| 23 | Windows setup installer works | **Successfully tested** (silent mode) | §4: install, files, uninstall entry, Start menu, e2e 13/13 against the installed exe, uninstall cleanup; silent install never starts the VB-CABLE download. The interactive wizard and its VB-CABLE task (UAC) weren't run, because they'd install a driver. |
| 24 | Windows portable works within documented requirements | **Successfully tested** | §4: data stays in `data\`, nothing in AppData/registry, e2e 13/13 with the portable exe. Documented: it needs VB-CABLE installed (admin) like the installer; it isn't driver-free. |
| 25 | Debian packaging works | **Successfully tested** | §7: `.deb` installs with apt on Debian 12 and 13, e2e 28/28 against `/usr/bin/freemic`, removes cleanly; lintian clean except an archive-upload note. |
| 26 | Arch packaging works | **Successfully tested** | §7: PKGBUILD → makepkg (with tests) → pacman -U → e2e 28/28 → pacman -R. |
| 27 | Linux portable works | **Successfully tested** | §7: tar.gz on Debian 12 (glibc 2.36), Debian 13 and Arch: e2e 28/28; data only in `./data`; Wayland 7/7. No AppImage (a plain archive was chosen; the virtual mic needs no bundled components). |
| 28 | Testing didn't disrupt normal computer usage | **Successfully tested** (procedures) | No default device changes, nothing played to speakers, physical mic never opened, no service restarts or reboots on the host; Windows tests abort if any app listens on CABLE Output; GUI windows off-screen. Linux tests ran in private servers inside separate WSL test distributions. One test distro (not your Ubuntu) had to be restarted with `wsl --terminate FreeMic-Debian` after a mount mistake inside it. |
| 29 | Performance measured and documented | **Successfully tested** | §8: 0.2.0 idle 0.040–0.047 % CPU, playing 0.047–0.38 %, memory 99–153 MB, on all three systems (0.1.0 numbers kept for comparison). |
| 30 | Visual design and theme completely remade, polished | **Implemented** | New dark and light themes (warm graphite / paper with a coral accent), Inter typeface, painted icons, sound pads showing length, loop and hotkey, header with page tabs and a status pill, bottom bar for playback and volumes, card-based settings, custom sliders and buttons, matching app icon. Screenshots in `docs/images/`. Whether it looks "human made" is your judgment. |
| 31 | No text overlapping | **Successfully tested** | §1b: every page, both themes, 9 window sizes, 100 % and 150 % scale, many states; checks text/text overlap, text cut off or outside the window, overlapping controls and text over controls. 5/5 on Windows, Debian, Arch. Real windows checked by eye in screenshots at 480×400, 520×420, 640×480, 900×600 and 1280×760. |
| 32 | Works when not in a full-screen window | **Successfully tested** | Minimum window 480×400; layouts adapt (header, bottom bar, settings rows, pad grid, YouTube rows) and every page scrolls; §1b sizes from 480×400 to 1920×1080. |
| 33 | YT-Downloader integrated: get sounds/music from YouTube in the app | **Successfully tested** | §1c on Windows, Arch, Debian 12; §6b GUI flow on Debian 13 and Arch (set up, look up, add, play into the virtual mic). Search, music query + ranking (verified against Python), player-client fallbacks and error messages ported from YT-Downloader; yt-dlp downloaded from its releases with SHA-256 verification and 6-hour update checks. |
| 34 | Downloads stored locally | **Successfully tested** | `sounds/youtube/<video id>.m4a` in FreeMic's data folder (portable: `data\`); §1c and §6b check the file, its decoding and that it's on the board. |
| 35 | Never downloads multiple videos from a list | **Successfully tested** | Playlist/channel links refused before yt-dlp runs; `list=` links reduced to the video id; yt-dlp gets only `watch?v=<id>` with `--no-playlist --max-downloads 1 --ignore-config`; result accepted only if exactly one file. Unit tests (§1), live test with a mix link (one file), GUI test (playlist link: error, nothing downloaded; mix link: exactly one file). Found while testing: `--no-playlist` alone doesn't stop a `/playlist?list=` link. |
| 36 | YouTube page on Windows | **Partially tested** | `youtube_live` on Windows (install, search, lookup, download, decode, cancel) and the page rendered on Windows with a live search (screenshot). Not clicked through with real mouse input on Windows, because that would inject input into your desktop; the same UI code was clicked through on Linux (§6b). |
| 37 | Packages for 0.2.0 | **Successfully tested** | Windows installer 15/15 and portable 9/9; `.deb` on Debian 12 and 13, PKGBUILD on Arch, Linux portable on Debian 13 and Arch: e2e 28/28 each (§4, §7). |

## Other requirements from the brief

| Requirement | Status |
|---|---|
| Architecture researched and documented with alternatives | Implemented: [ARCHITECTURE.md](ARCHITECTURE.md) |
| Physical microphone selection | Implemented (Settings → Your microphone); Linux e2e uses a selected fake mic; Windows capture test selects a device by id |
| Optional monitoring without feedback | Implemented (soundboard only, off by default, explicit device). Not tested with a real output device, since that would play to your speakers or headphones |
| Global hotkeys | Successfully tested on Linux/X11 (start + stop hotkeys); implemented on Windows, not tested (§9); not available on Wayland (documented; `freemic --play` instead) |
| Start with the system | Implemented, not tested (would change your startup entries) |
| Clear error messages for bad files | Successfully tested (format tests: empty/truncated/corrupt/not-audio/Opus) |
| Licenses reviewed | Implemented: [THIRD-PARTY-NOTICES.md](../THIRD-PARTY-NOTICES.md); all permissive except Symphonia (MPL-2.0, file-level copyleft, unmodified); VB-CABLE not redistributed |
| Documentation (all 15 topics in the brief) | Implemented: README, INSTALL, USER-GUIDE, ARCHITECTURE, TROUBLESHOOTING, BUILDING, TESTING, this checklist, man page; updated for 0.2.0 (YouTube page, new layout, fonts, yt-dlp) |

## What needs your permission to finish testing

1. Opening your **physical microphone** (Logitech G432) in FreeMic for a few seconds, to confirm capture with real hardware.
2. A **Discord** test: you select `CABLE Output` as Discord's input (e.g. in a test server, or Discord's "Let's Check" mic test) and listen to the playback.
3. **Windows global hotkeys**: register a test combination and press it yourself (or allow injected key presses while you're away).
4. Optionally, **unplugging/replugging** a microphone while FreeMic runs.
