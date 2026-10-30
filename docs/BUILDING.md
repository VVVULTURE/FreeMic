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
time, so their development packages aren't needed for building.

## Build and run

```sh
cargo build --release
./target/release/freemic          # Windows: target\release\freemic.exe
cargo test --release              # unit, format and pipeline tests (no audio devices used)
```

`src/bin/fmtest.rs` is a developer tool built alongside it (`fmtest list`,
`record`, `analyze`, `tone`, `icons`), used by the end-to-end tests.

## Release packages

### Windows

```powershell
powershell -ExecutionPolicy Bypass -File packaging\windows\build.ps1
```

Produces `dist\FreeMic-<ver>-windows-setup.exe` (Inno Setup,
`packaging\windows\freemic.iss`) and `dist\FreeMic-<ver>-windows-portable.zip`.
The executable gets its icon and version information from `build.rs`
(`packaging/assets/freemic.ico`). `tools/windows/test_installer.ps1` tests
the installer (see [TESTING.md](TESTING.md)).

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

## Third-party notices

`THIRD-PARTY-NOTICES.md` is generated from `cargo metadata` by
`tools/third_party.py`. Re-run it after changing dependencies.
