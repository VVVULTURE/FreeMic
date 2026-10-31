## What's new in 0.2.1: security fix for the Windows antivirus detection

Microsoft Defender flagged FreeMic 0.2.0 as `Trojan:Win32/Bearfoos.B!ml` (a machine-learning
detection, not a known virus) and removed it. It reacted the moment FreeMic saved the
yt-dlp program it had downloaded: an unknown program that downloads and saves an executable
looks like a malware "dropper". 0.2.1 removes everything that looks like malware:

- **On Windows, FreeMic never downloads or writes program files.** yt-dlp now comes
  with FreeMic (the official `yt-dlp.exe`, checked against its published SHA-256 when the
  package is built). **Update yt-dlp** uses yt-dlp's own updater, and only when you click it.
- **No PowerShell, no scripts, no administrator rights.** The VB-CABLE helper script is gone:
  FreeMic and the installer now open the VB-CABLE web page, and you run VB-Audio's own
  installer. FreeMic picks up the new device by itself within a few seconds.
- yt-dlp is stopped through a Windows job object instead of `taskkill`, so it can never
  outlive FreeMic.
- `FreeMic.exe` carries full version information and an application manifest (`asInvoker`).
- Tested on Windows 11 with Microsoft Defender running: package scans and a real YouTube
  search, download and update produced no detection.

Also fixed: switching pages no longer keeps the scroll position of the previous page.

If you had 0.2.0: leave its quarantined files in Windows Security, install 0.2.1, and your
soundboard and settings are kept. 0.2.1 deletes the yt-dlp copy 0.2.0 had downloaded.

## Downloads

| System | File |
|---|---|
| Windows 10/11 (installer) | `FreeMic-*-windows-setup.exe` |
| Windows (portable) | `FreeMic-*-windows-portable.zip` |
| Debian 12+ / Ubuntu 22.04+ | `freemic_*_amd64.deb` (`sudo apt install ./freemic_*_amd64.deb`) |
| Arch Linux | `freemic-*-x86_64.pkg.tar.zst` (`sudo pacman -U ...`), or build with `PKGBUILD` + `freemic-*.tar.gz` |
| Other Linux (glibc 2.35+) | `freemic-*-linux-x86_64.tar.gz` (extract and run `./freemic`) |

**Windows needs the free VB-CABLE driver** for the virtual microphone (it isn't bundled).
FreeMic's Settings page and the installer's last page open VB-Audio's download page;
installing the driver needs administrator approval. In Discord/games choose **CABLE Output**
as the microphone. On Linux choose **FreeMic Virtual Microphone**.

The Windows files aren't code-signed, so SmartScreen may ask you to confirm the first start
(**More info → Run anyway**). The Windows packages include yt-dlp, whose standalone build is
GPL-3.0-or-later; its licenses and source location ship next to it.

See [INSTALL.md](https://github.com/VVVULTURE/FreeMic/blob/main/docs/INSTALL.md) and the
[user guide](https://github.com/VVVULTURE/FreeMic/blob/main/docs/USER-GUIDE.md).
