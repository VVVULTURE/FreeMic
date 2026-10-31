## What's new in 0.2.0

- **New look.** A redesigned interface with its own dark and light themes, the Inter
  typeface, painted icons, sound pads that show length, loop and hotkey, and a bottom bar
  for playback and volumes. It adapts to any window size from 480 × 400 up; automated
  tests check every page at nine window sizes for overlapping or cut-off text.
- **Sounds from YouTube.** The new YouTube page (based on YT-Downloader) searches YouTube,
  or takes a pasted link, and saves the audio of **one** video into your sound library and
  onto the board. Music search ranks results like YT-Downloader. Playlists are never
  downloaded. yt-dlp is fetched from its official GitHub releases on first use and checked
  against its published SHA-256.

## Downloads

| System | File |
|---|---|
| Windows 10/11 (installer) | `FreeMic-*-windows-setup.exe` |
| Windows (portable) | `FreeMic-*-windows-portable.zip` |
| Debian 12+ / Ubuntu 22.04+ | `freemic_*_amd64.deb` (`sudo apt install ./freemic_*_amd64.deb`) |
| Arch Linux | `freemic-*-x86_64.pkg.tar.zst` (`sudo pacman -U ...`), or build with `PKGBUILD` + `freemic-*.tar.gz` |
| Other Linux (glibc 2.35+) | `freemic-*-linux-x86_64.tar.gz` (extract and run `./freemic`) |

**Windows needs the free VB-CABLE driver** for the virtual microphone (it isn't bundled). The
installer or FreeMic's Settings page can download VB-Audio's official installer for you;
installing it needs administrator approval. In Discord/games choose **CABLE Output** as the
microphone. On Linux choose **FreeMic Virtual Microphone**.

See [INSTALL.md](https://github.com/VVVULTURE/FreeMic/blob/main/docs/INSTALL.md) and the
[user guide](https://github.com/VVVULTURE/FreeMic/blob/main/docs/USER-GUIDE.md).
