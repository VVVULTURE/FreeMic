# Troubleshooting

First look at the status bar at the bottom of the FreeMic window and at
**Settings → Virtual microphone**. They say what FreeMic is doing and which
device to select in other programs. For details, **Settings → Open log
file** (`freemic.log`; the previous run's log is kept as `freemic.old.log`).

## Others can't hear the soundboard

1. In the other program, the input device must be the **virtual**
   microphone: `CABLE Output (VB-Audio Virtual Cable)` on Windows, `FreeMic
   Virtual Microphone` on Linux. Your real headset microphone won't carry
   the sounds.
2. The status bar must say **Virtual mic active**. If it says *unavailable*,
   read the message (Windows: usually VB-CABLE isn't installed yet).
3. Check **Sounds** and **Virtual microphone output** volumes, and watch
   the meters in Settings → Volume while a sound plays.
4. Discord: turn off **Noise Suppression** (Krisp) and lower **Input
   Sensitivity**, or use Push-to-talk. Noise suppression is designed to
   remove non-voice sounds and will cut music and sound effects. Also check
   that Discord's input volume isn't at 0.
5. Windows: in **Sound settings → Input → CABLE Output**, make sure the
   device isn't disabled and its level isn't 0.

## Others can't hear my voice (but they hear sounds)

- Settings → Your microphone → **Pass my microphone through** must be on,
  and **Mute mic** off.
- Pick your real microphone there instead of `System default` if the
  default is something else.
- Windows: **Settings → Privacy & security → Microphone → Let desktop apps
  access your microphone** must be on.
- The line under the microphone selector shows an error if the device
  couldn't be opened, e.g. if another program uses it in exclusive mode.

## Windows: "No virtual microphone found" / VB-CABLE issues

- Install VB-CABLE (Settings → **Download and install VB-CABLE…**, or see
  [INSTALL.md](INSTALL.md#windows-vb-cable)), restart if asked, then click
  **Re-check devices**.
- If the automatic download fails (no internet, proxy), download the zip
  from <https://vb-audio.com/Cable/> yourself.
- "Signature check failed": the downloaded file isn't signed by VB-Audio.
  FreeMic refuses to run it. Download it manually from the official site.
- Everything you hear goes into the microphone: **CABLE Input** was made the
  Windows default output device. Set your speakers/headphones back as the
  default output. FreeMic never changes this setting itself.
- You hear yourself or the soundboard twice: in Windows sound settings,
  **CABLE Output → Properties → Listen → "Listen to this device"** must be
  off. Use FreeMic's own monitoring instead (soundboard only).

## Linux: no "FreeMic Virtual Microphone"

- FreeMic needs a PulseAudio-compatible server for your user:
  `pactl info` must work. On PipeWire systems install and start
  `pipewire-pulse` (Arch: `sudo pacman -S pipewire-pulse`, then log out and
  in, or `systemctl --user enable --now pipewire-pulse.socket`).
- `pactl list short sources | grep freemic` should show `freemic_mic` while
  FreeMic runs.
- Flatpak/Snap apps (e.g. Discord as Flatpak) see the same devices, as long
  as they have the usual audio permission.
- Some apps list only one microphone by default. Pick FreeMic's in the
  app's own settings, or in `pavucontrol` → *Recording* while the app
  records.
- After a crash the old devices remain until FreeMic starts again. To
  remove them by hand, see [INSTALL.md](INSTALL.md#leftover-devices-after-a-crash-linux).

## Hotkeys don't work

- Settings → Hotkeys lists combinations that couldn't be registered because
  another program owns them. Choose a different combination.
- **Wayland** doesn't allow global hotkeys for apps. Settings says so. Bind
  keys in your desktop settings to `freemic --play "Name"` / `freemic --stop`.
- Some games running as administrator receive keys before normal programs
  on Windows. If hotkeys don't work only inside such a game, run FreeMic as
  administrator too, or use a different key.

## Sound quality

- **Crackles or dropouts**: look for `underrun` lines in the log. Close
  other heavy programs, and on laptops use a power plan that doesn't park
  the CPU. FreeMic recovers automatically after an underrun.
- **Sounds too quiet or too loud** compared to your voice: change
  **Sounds** vs **Mic** in the top bar. Normalize very quiet or very loud
  clips in an audio editor.
- **"Peak limiter is working"**: the mix is near full scale and the limiter
  is reducing peaks. Lower **Sounds** or **Mic** a little for a cleaner
  sound.
- **Echo of my voice**: someone in the call has speakers that pick you up,
  or "Listen to this device" is on (see above). FreeMic's monitor never
  plays your own microphone.

## A file won't import or play

- Ogg Opus, WMA and DRM-protected files aren't supported. Convert them, e.g.
  `ffmpeg -i in.opus out.ogg`.
- "contains no audio" / "Can't play": the file is damaged or empty. FreeMic
  shows the reason in a message.
- A sound whose original file was moved shows an error when played (only if
  "Copy added files into FreeMic's library" was off). Right-click →
  **Replace audio file…**.

## YouTube page

- **"Could not reach GitHub" during setup**: FreeMic downloads yt-dlp from
  github.com over HTTPS. Check your internet connection, proxy or firewall.
  Once yt-dlp is installed, a failed update check doesn't stop it from
  working.
- **"failed its checksum"**: the downloaded yt-dlp didn't match the SHA-256
  published with the release, so nothing was installed. Try again; if it
  keeps happening, something between you and GitHub is changing downloads.
- **"YouTube blocked this attempt (bot check or session error)"**: YouTube
  sometimes refuses automated requests. Wait a moment and try again. If it
  keeps happening, use **Settings → YouTube downloads → Check for update**;
  new yt-dlp releases follow YouTube's changes. FreeMic already tries five
  different YouTube player clients before giving up.
- **"YouTube offers no AAC audio for this video"**: FreeMic plays the AAC
  stream directly and doesn't convert audio. Try another upload of the same
  sound.
- **"This link isn't a single video"**: FreeMic only downloads one video at a
  time. Open the video itself and copy its link (it contains `watch?v=`,
  `youtu.be/` or `/shorts/`).
- **"private, members-only or has been removed"**: the video can't be
  downloaded.
- Details of each attempt (the yt-dlp error line) are in `freemic.log`.

## FreeMic is already running

Only one FreeMic per user runs at a time. Look for its window in the taskbar,
or close it with `freemic --quit`.

## Reset everything

Close FreeMic and delete (or rename) its data folder (see
[INSTALL.md](INSTALL.md#where-freemic-stores-data)). On the next start
FreeMic uses default settings and an empty soundboard.

## Reporting a bug

Attach `freemic.log` (Settings → Open log file). It contains device names
and error messages but no audio. If audio drift or dropouts are involved,
start FreeMic with the environment variable `FREEMIC_STATS=1` and attach
the log from that session.
