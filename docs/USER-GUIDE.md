# FreeMic user guide

![Main window](images/main-window.png)

## The idea

```
 your microphone ──┐
                   ├─► FreeMic mix ─► virtual microphone ─► Discord / game / OBS
 soundboard ───────┘          (limiter)
```

Other programs never use your real microphone directly. They use the
virtual microphone, which carries your voice plus whatever the soundboard
plays. Select it once as the input device in each program:

- **Windows:** `CABLE Output (VB-Audio Virtual Cable)`
- **Linux:** `FreeMic Virtual Microphone`

The status bar at the bottom of the window always shows the name to pick and
whether your microphone is being passed through.

## Adding sounds

- Click **+ Add sounds** and choose one or more files, or drag files from
  your file manager onto the window.
- Supported: MP3, WAV (8/16/24-bit, 32-bit float, 8–96 kHz, mono to 5.1),
  OGG Vorbis, FLAC, M4A/MP4/AAC (AAC-LC and ALAC). Files are converted to the
  output's sample rate as they play. Surround files are mixed down to stereo.
- Not supported: Ogg **Opus**, WMA, and DRM-protected files. FreeMic tells
  you which files it couldn't import and why.
- By default the files are **copied** into FreeMic's library, so the
  soundboard keeps working if you move or delete the originals. Turn this
  off in Settings → General if you'd rather keep only links.

## Playing

| Action | How |
|---|---|
| Play a sound | Click its button, or press its hotkey |
| Stop it | Click the playing button again, or **Stop**, or the stop hotkey |
| Play another one | Click it: the current sound stops at once (short fade) and the new one starts |

Only one sound plays at a time. The playing button gets a green outline and
a progress bar, and the top bar shows `Playing:` or `Looping:` with the time.

Each sound has a mode (right-click → **Play once** / **Loop until
stopped**). Looping sounds show a `LOOP` badge and repeat seamlessly until
you stop them. **Settings → General → New sounds play** sets the mode for
newly added sounds.

## Organizing

Right-click a button for:

- **Rename…**
- **Play once / Loop until stopped**
- a color (or **Default color**)
- **Set hotkey… / Clear hotkey**
- **Replace audio file…**: keeps the button, name, color and hotkey
- **Move left / Move right**: or simply drag the button to a new place
- **Remove…**: asks first; the copied file is deleted from the library

With more than 12 sounds a **Filter** box appears above the grid (dragging
is turned off while filtering). **Settings → General → Button size** changes
the size of the buttons.

## Volumes

| Control | Where | Range | Affects |
|---|---|---|---|
| Sounds | top bar, Settings → Volume | 0–100 % | soundboard level in the mix |
| Mic | top bar, Settings → Your microphone | 0–200 % | your voice level in the mix |
| Virtual microphone output | Settings → Volume | 0–100 % | the whole mix |
| Monitor volume | Settings → Hear the soundboard yourself | 0–100 % | only what you hear |

All volumes are saved. The sliders follow a loudness-like curve (50 % is
about −12 dB). The **Mute mic** button silences your voice while sounds keep
playing.

The **Volume** section shows live meters for microphone, soundboard and
output. A peak limiter keeps the mix just below full scale so it never
clips. If it has to work hard, Settings shows "Peak limiter is working…";
lower a volume then, because heavy limiting sounds squashed.

## Your microphone

**Settings → Your microphone**:

- **Pass my microphone through to the virtual microphone**: on by default.
  Turn it off to send only the soundboard.
- **Microphone**: `System default` follows your Windows/desktop default
  microphone, or choose a specific one. The virtual microphone itself is
  never offered here, which prevents feedback loops.
- The line below shows the device, its sample rate and FreeMic's buffer
  size. FreeMic keeps about 30 ms of buffer for your voice and corrects
  small clock differences between the microphone and the output, so the mix
  stays in sync even after hours.

If you chose a specific microphone and it's unplugged, your voice drops out
of the mix, the soundboard keeps working, and FreeMic reconnects when that
microphone comes back. It doesn't switch to a different microphone on its
own. With `System default`, FreeMic follows whatever your system's default
microphone is.

## Hearing the soundboard yourself (monitoring)

Off by default. **Settings → Hear the soundboard yourself → Monitor on**:
pick your headphones. Only the soundboard is played there, **never your own
microphone**, so there's no echo of your voice. FreeMic never plays anything
to a device you didn't pick here.

## Hotkeys

- Per sound: right-click → **Set hotkey…**, then press the combination.
  Letters, digits and most other keys need Ctrl, Alt or Shift (so typing in
  a chat can't trigger sounds). F1–F20, Insert, Home, End, Page Up and Page
  Down can be used alone. Escape cancels.
- Stop all sounds: **Settings → Hotkeys → Stop all sounds → Set…**
- Hotkeys work while other programs and games have focus. If a combination
  is already taken by another program, Settings says so; choose another.
- **Wayland (Linux):** programs can't grab global keys there. Bind keys in
  your desktop's keyboard settings to commands instead:

  ```
  freemic --play "Airhorn"     # by name, or --play "#3" for the third button
  freemic --stop
  ```

  `--play` on a sound that's already playing stops it, like clicking. The
  same commands work on Windows and X11, for example from a Stream Deck or
  AutoHotkey.

## Virtual microphone settings

![Settings](images/settings.png)

**Settings → Virtual microphone** shows where the mix goes and which name
other programs should select.

- **Windows:** `Automatic (detect virtual cable)` uses VB-CABLE's `CABLE
  Input`. If no cable is installed, the page explains how to set one up and
  offers **Download and install VB-CABLE…**. You can also choose any output
  device by hand, for example another virtual cable. **Re-check devices**
  looks again after you've installed a driver.
- **Linux:** FreeMic creates the virtual microphone itself through
  PulseAudio/PipeWire when it starts, and removes it when it quits.

## General settings

- **Theme**: System, Dark or Light.
- **Start FreeMic when I sign in (minimized)**: Windows registry Run entry,
  or `~/.config/autostart/freemic.desktop` on Linux.
- **Open data folder / Open log file**: useful for backups and bug reports.

## Data, backup and moving to another PC

Everything is in one folder (see [INSTALL.md](INSTALL.md#where-freemic-stores-data)):
`settings.json`, `soundboard.json` and `sounds/`. To back up or move your
soundboard, copy that folder while FreeMic is closed. With the portable
version, just copy the whole FreeMic folder.
