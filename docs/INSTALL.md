# Installing, uninstalling and cleaning up FreeMic

- [Windows: installer](#windows-installer)
- [Windows: portable](#windows-portable)
- [Windows: VB-CABLE (the virtual microphone driver)](#windows-vb-cable)
- [Debian / Ubuntu (.deb)](#debian--ubuntu)
- [Arch Linux (PKGBUILD)](#arch-linux)
- [Linux portable](#linux-portable)
- [Where FreeMic stores data](#where-freemic-stores-data)

Tested on: Windows 11 Pro (build 26200) with VB-CABLE; Debian 13
(PulseAudio 17), Debian 12 (PulseAudio 16) and Arch Linux (PipeWire 1.6.10).
Windows 10 and Ubuntu 22.04+ should work (same APIs; Ubuntu 22.04 has glibc
2.35, the minimum) but haven't been tested.

---

## Windows installer

1. Run `FreeMic-0.2.1-windows-setup.exe`. It installs for your user account
   only (into `%LOCALAPPDATA%\Programs\FreeMic`) and doesn't need
   administrator rights. It contains FreeMic and yt-dlp (for the YouTube
   page); nothing is downloaded during setup.
   FreeMic isn't code-signed, so Windows SmartScreen may show "Windows
   protected your PC" for a new download: **More info** → **Run anyway**.
2. If no VB-CABLE driver is found, the last page has a checkbox **Open the
   VB-CABLE web page**. Install VB-CABLE from there with VB-Audio's own
   installer (see [Windows: VB-CABLE](#windows-vb-cable)).
3. Start FreeMic from the Start menu.

Upgrading: run the new installer; your soundboard and settings are kept.

Silent install (IT/scripts):
`FreeMic-0.2.1-windows-setup.exe /VERYSILENT /SUPPRESSMSGBOXES /NORESTART`
(add `/DIR="C:\path"` to choose the folder). Silent installs don't open the
web page.

### Uninstall

**Settings → Apps → Installed apps → FreeMic → Uninstall**, or Start menu →
FreeMic → **Uninstall FreeMic**. The uninstaller:

- removes the program, its Start menu entries and the "start when I sign in"
  entry (`HKCU\Software\Microsoft\Windows\CurrentVersion\Run\FreeMic`);
- asks whether to delete your soundboard, imported sounds and settings
  (`%APPDATA%\FreeMic`). In silent mode (`unins000.exe /VERYSILENT`) they're
  kept; delete that folder yourself if you want them gone;
- **doesn't** remove VB-CABLE, because other programs might use it (see
  below).

## Windows portable

1. Extract `FreeMic-0.2.1-windows-portable.zip` anywhere, including a USB
   stick. Keep `yt-dlp.exe` next to `FreeMic.exe` (the YouTube page uses it).
2. Run `FreeMic.exe`.

The included `portable.txt` keeps all settings, the soundboard and imported
sounds in the `data` folder next to `FreeMic.exe`. Nothing is written to
`%APPDATA%` or the registry, except the optional "start when I sign in"
setting, which has to live in the registry.

**The portable version still needs VB-CABLE.** A driver can't be "portable":
installing one always needs administrator approval on each PC. **Settings →
Open the VB-CABLE page** takes you to VB-Audio's download.

To remove the portable version: if you turned on "Start FreeMic when I sign
in", turn it off first. Then delete the folder. Nothing else is left behind.

## Windows: VB-CABLE

FreeMic plays the mix into **CABLE Input**, and VB-CABLE presents it as the
microphone **CABLE Output (VB-Audio Virtual Cable)**.

FreeMic doesn't download or start driver installers itself (that needs your
administrator approval, in VB-Audio's own installer). To install VB-CABLE:

1. Open <https://vb-audio.com/Cable/> (Settings → **Open the VB-CABLE page**)
   and download the Windows package (a zip, e.g. `VBCABLE_Driver_Pack45.zip`).
2. Extract it, right-click `VBCABLE_Setup_x64.exe` → **Run as administrator**
   → **Install Driver**.
3. Restart Windows if asked. FreeMic checks for the device every 3 seconds
   and starts using it by itself (**Re-check devices** checks right away).

FreeMic never changes your Windows default microphone or speakers. Leave them
as they are. In particular, **don't** make "CABLE Input" your default output,
or every Windows sound would go into your microphone.

Removing VB-CABLE: run `VBCABLE_Setup_x64.exe` again as administrator and
click **Remove Driver**, then restart Windows.

Other virtual cables: FreeMic also recognizes VB-Audio's A/B/C/D cables
("CABLE-A Input"...) and the commercial "Virtual Audio Cable" ("Line 1") by
name, but only the standard VB-CABLE has been tested. Any output device can
be chosen by hand under **Settings → Send the mix to**.

---

## Debian / Ubuntu

```sh
sudo apt install ./freemic_0.2.1-1_amd64.deb
```

`apt` installs the needed libraries (libpulse0, X11/Wayland and GL
libraries). FreeMic needs a running PulseAudio, or PipeWire with
`pipewire-pulse`. Desktop installs normally have one of them; the package
recommends both alternatives.

Start it from the application menu, or run `freemic`.

Uninstall: `sudo apt remove freemic`. Your settings stay in your home
directory; to delete them as well:

```sh
rm -rf ~/.config/freemic ~/.local/share/freemic ~/.config/autostart/freemic.desktop
```

## Arch Linux

Prebuilt package from the release page:

```sh
sudo pacman -U freemic-0.2.1-1-x86_64.pkg.tar.zst
```

Or build it yourself: put `PKGBUILD` and `freemic-0.2.1.tar.gz` in one folder and run:

```sh
makepkg -si
```

This builds FreeMic from source (needs `cargo`), runs its test suite and
installs it. On PipeWire systems make sure `pipewire-pulse` is installed (it
is on standard desktop installs).

Uninstall: `sudo pacman -R freemic`, then optionally
`rm -rf ~/.config/freemic ~/.local/share/freemic ~/.config/autostart/freemic.desktop`.

## Linux portable

```sh
tar xzf freemic-0.2.1-linux-x86_64.tar.gz
cd freemic-0.2.1-linux-x86_64
./freemic
```

Needs glibc 2.35 or newer and the usual desktop libraries (libpulse, libX11,
libXcursor, libXi, libxkbcommon, libGL/libEGL, libwayland on Wayland). Data
is kept in the `data` folder next to the program because of `portable.txt`.
No root rights are needed, because the virtual microphone is created through
your own sound server and removed when FreeMic exits. To remove it, delete
the folder (and `~/.config/autostart/freemic.desktop` if you enabled
"start when I sign in").

### Leftover devices after a crash (Linux)

FreeMic creates two PulseAudio modules, the sink `freemic_sink` and the
source `freemic_mic`, and removes them on exit. If FreeMic is killed, they
stay until FreeMic starts again (it cleans them up at start-up) or until
you log out. To remove them by hand:

```sh
pactl list short modules | grep freemic          # shows the module numbers
pactl unload-module <number>                      # for each line
```

---

## Where FreeMic stores data

| | Settings (`settings.json`, `soundboard.json`) | Sounds (`sounds/`, YouTube audio in `sounds/youtube/`), `tools/` and `freemic.log` |
|---|---|---|
| Windows (installed) | `%APPDATA%\FreeMic` | `%APPDATA%\FreeMic` |
| Linux (installed) | `~/.config/freemic` | `~/.local/share/freemic` |
| Portable (`portable.txt` or a `data` folder next to the program) | `data\` | `data\` |

`tools/yt-dlp.json` remembers when FreeMic last looked for a newer yt-dlp.
On **Windows**, yt-dlp itself (`yt-dlp.exe`) is part of FreeMic and sits next
to `FreeMic.exe`; uninstalling FreeMic removes it. (FreeMic 0.2.0 had
downloaded it into `tools\yt-dlp.exe`; 0.2.1 deletes that copy.) On
**Linux**, yt-dlp is downloaded into `tools/yt-dlp_linux` the first time you
use the YouTube page; removing FreeMic's data folder removes it too, and
**Settings → YouTube downloads → Remove yt-dlp** deletes it on its own.

If a settings file is damaged, FreeMic renames it to
`*.damaged-<time>.json`, starts with defaults and tells you.
