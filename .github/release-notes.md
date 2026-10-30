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
