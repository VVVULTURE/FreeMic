# Building FreeMic

## Requirements

- Rust 1.99 or newer (`rustup`, stable toolchain). The project uses edition
  2024 and `Cargo.lock` pins all dependencies.
- **Windows:** Visual Studio Build Tools (MSVC, "Desktop development with
  C++"). For the installer: [Inno Setup 6](https://jrsoftware.org/isinfo.php)
  (`ISCC.exe`).
- **Debian/Ubuntu:** `sudo apt install build-essential pkg-config libpulse-dev`
- **Arch:** `sudo pacman -S --needed base-devel rust libpulse`

The GUI libraries (X11, Wayland, xkbcommon, OpenGL/EGL) are loaded at run
time, so their development packages aren't needed for building. HTTPS for
the YouTube page uses rustls with the `ring` crypto backend, which compiles
some C and assembly with the C compiler listed above (no OpenSSL needed).

`assets/fonts/InterVariable.ttf` (the UI font) is embedded into the program
at compile time; the source tarball and packaging scripts include `assets/`.

## Build and run

```sh
cargo build --release
./target/release/freemic          # Windows: target\release\freemic.exe
cargo test --release              # unit, format, pipeline and UI layout tests (no audio devices used)
cargo test --release --test youtube_live -- --ignored --nocapture --test-threads 1   # live YouTube tests (need the internet)
```

On Windows the live tests need an official `yt-dlp.exe`: set
`FREEMIC_YTDLP` to one (e.g. `dist\yt-dlp\<version>\yt-dlp.exe` from the
package build), and for `yt_dlp_self_update` set `FREEMIC_YTDLP_OLD` to an
older official release. A development build looks for `yt-dlp.exe` next to
itself; set `FREEMIC_YTDLP` to use the YouTube page from `target\release`.

`src/bin/fmtest.rs` is a developer tool built alongside it (`fmtest list`,
`record`, `analyze`, `tone`, `icons`), used by the end-to-end tests.

## Release packages

### Windows

```powershell
powershell -ExecutionPolicy Bypass -File packaging\windows\build.ps1
```

Produces `dist\FreeMic-<ver>-windows-setup.exe` (Inno Setup,
`packaging\windows\freemic.iss`) and `dist\FreeMic-<ver>-windows-portable.zip`.
The executable gets its icon, version information and application manifest
(`packaging/windows/freemic.manifest`: `asInvoker`, Windows 10/11) from
`build.rs`.

Both packages include the official `yt-dlp.exe`. The script downloads it
from the newest yt-dlp release (or `-YtDlpVersion 2026.10.26`), refuses it
unless it matches the SHA-256 in that release's `SHA2-256SUMS`, and adds
yt-dlp's `LICENSE`, `THIRD_PARTY_LICENSES.txt` and a source note
(`yt-dlp-SOURCE.txt`): the standalone build as a whole is GPL-3.0-or-later,
so its license texts and source location must ship with it. Downloads are
kept in `dist\yt-dlp\<version>\`.

`tools/windows/test_installer.ps1`, `test_portable.ps1` and
`youtube_test.ps1` test the packages (see [TESTING.md](TESTING.md)).

### Linux

```sh
bash packaging/linux/build.sh
```

Produces in `dist/linux/`:

- `freemic_<ver>-1_amd64.deb` (built with `dpkg-deb`)
- `freemic-<ver>-linux-x86_64.tar.gz` (portable, includes `portable.txt`)
- `freemic-<ver>.tar.gz` + `PKGBUILD` (Arch source package; the script fills
  in the checksum)

The binary needs the glibc of the build system or newer. For wide
compatibility the official artifacts are built in a **Debian 12
(bookworm)** chroot, which gives glibc 2.36 and a binary that needs only
GLIBC_2.35 symbols. Built on newer systems, Rust's standard library also
records a weak `GLIBC_2.39` reference (`pidfd_spawnp`), which makes older
loaders print a warning.

```sh
# once, as root, in a Debian/Ubuntu VM or container:
debootstrap --variant=minbase bookworm /srv/bookworm http://deb.debian.org/debian
bash tools/linux/setup_bookworm_builder.sh /srv/bookworm   # build tools + Rust
# each release:
bash tools/linux/build_in_chroot.sh /srv/bookworm "$PWD" "$PWD/dist/linux"
```

The Arch package is built on Arch from the source tarball with `makepkg`.
`makepkg` also runs `cargo test`.

## Icons

`packaging/assets/` holds the generated icons (`freemic-16…256.png`,
`freemic.ico`). They're drawn by `src/icon.rs`, the same code that draws the
window icon. Regenerate them with `cargo run --release --bin fmtest -- icons
packaging/assets`.

## Fonts

`assets/fonts/InterVariable.ttf` and `Inter-LICENSE.txt` come unmodified
from the official [Inter 4.1 release](https://github.com/rsms/inter/releases/tag/v4.1)
(`Inter-4.1.zip`, SHA-256
`9883fdd4a49d4fb66bd8177ba6625ef9a64aa45899767dde3d36aa425756b11e`).

## Third-party notices

`THIRD-PARTY-NOTICES.md` is generated from `cargo metadata` by
`tools/third_party.py`. Re-run it after changing dependencies.
