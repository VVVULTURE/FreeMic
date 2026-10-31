#!/bin/bash
# Build the Linux release artifacts from a source checkout (run on Debian/Ubuntu):
#   dist/linux/freemic_<ver>-1_amd64.deb            Debian/Ubuntu package
#   dist/linux/freemic-<ver>-linux-x86_64.tar.gz    portable archive
#   dist/linux/freemic-<ver>.tar.gz + PKGBUILD      Arch Linux source package
# Needs: cargo, dpkg-deb, gzip.
set -euo pipefail
cd "$(dirname "$0")/../.."
ROOT=$PWD
VER=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
OUT=$ROOT/dist/linux
rm -rf "$OUT"; mkdir -p "$OUT"
cargo build --release --locked --bin freemic
BIN=$ROOT/target/release/freemic

# --- source tarball for the PKGBUILD -----------------------------------------
SRC=$OUT/src/freemic-$VER
mkdir -p "$SRC"
cp -r Cargo.toml Cargo.lock build.rs assets src tests tools docs packaging LICENSE README.md THIRD-PARTY-NOTICES.md "$SRC/"
# Normalise permissions (a checkout on a Windows drive shows every file as 0777).
find "$SRC" -type d -exec chmod 755 {} +
find "$SRC" -type f -exec chmod 644 {} +
find "$SRC" -type f -name '*.sh' -exec chmod 755 {} +
tar -C "$OUT/src" --sort=name --owner=0 --group=0 -czf "$OUT/freemic-$VER.tar.gz" "freemic-$VER"
rm -rf "$OUT/src"
SUM=$(sha256sum "$OUT/freemic-$VER.tar.gz" | cut -d' ' -f1)
sed -e "s/@SHA256@/$SUM/" -e "s/^pkgver=.*/pkgver=$VER/" packaging/linux/arch/PKGBUILD > "$OUT/PKGBUILD"

# --- .deb --------------------------------------------------------------------
PKG=$OUT/deb
install -Dm755 "$BIN" "$PKG/usr/bin/freemic"
install -Dm644 packaging/linux/freemic.desktop "$PKG/usr/share/applications/freemic.desktop"
mkdir -p "$PKG/usr/share/man/man1"; gzip -9n < packaging/linux/freemic.1 > "$PKG/usr/share/man/man1/freemic.1.gz"
for s in 16 24 32 48 64 128 256; do
  install -Dm644 "packaging/assets/freemic-$s.png" "$PKG/usr/share/icons/hicolor/${s}x${s}/apps/freemic.png"
done
DOC=$PKG/usr/share/doc/freemic
install -Dm644 README.md "$DOC/README.md"
install -Dm644 THIRD-PARTY-NOTICES.md "$DOC/THIRD-PARTY-NOTICES.md"
install -Dm644 assets/fonts/Inter-LICENSE.txt "$DOC/Inter-LICENSE.txt"
for f in docs/*.md docs/images/*.png; do  # user documentation (the development work log stays out)
  [ "$(basename "$f")" = WORKLOG.md ] || install -Dm644 "$f" "$DOC/$f"
done
{
  echo "Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/"
  echo "Upstream-Name: FreeMic"
  echo
  echo "Files: *"
  echo "Copyright: 2026 Vaden"
  echo "License: MIT"
  sed 's/^$/./; s/^/ /' LICENSE
  echo
  echo "Comment: The binary statically includes Rust crates under MIT, Apache-2.0, BSD,"
  echo " Zlib, Unicode, OFL (fonts) and MPL-2.0 (Symphonia) licenses; see"
  echo " /usr/share/doc/freemic/THIRD-PARTY-NOTICES.md. The embedded Inter font is"
  echo " licensed under the SIL Open Font License 1.1: /usr/share/doc/freemic/Inter-LICENSE.txt."
} > "$DOC/copyright"
printf 'freemic (%s-1) unstable; urgency=medium\n\n  * Release %s.\n\n -- Vaden <vadenjonas@gmail.com>  %s\n' \
  "$VER" "$VER" "$(date -R)" | gzip -9n > "$DOC/changelog.Debian.gz"
mkdir -p "$PKG/DEBIAN"
SIZE=$(du -sk --exclude=DEBIAN "$PKG" | cut -f1)
cat > "$PKG/DEBIAN/control" <<CTL
Package: freemic
Version: $VER-1
Architecture: amd64
Maintainer: Vaden <vadenjonas@gmail.com>
Installed-Size: $SIZE
Section: sound
Priority: optional
Depends: libc6 (>= 2.35), libgcc-s1, libpulse0, libx11-6, libx11-xcb1, libxcursor1, libxi6, libxkbcommon0, libxkbcommon-x11-0, libgl1, libegl1, libwayland-client0, libwayland-cursor0, libwayland-egl1
Recommends: pipewire-pulse | pulseaudio
Description: soundboard and virtual microphone
 FreeMic plays sound clips into a virtual microphone ("FreeMic Virtual
 Microphone") together with your real microphone, so voice chat and games
 hear both. Works with PulseAudio and with PipeWire (pipewire-pulse). The
 virtual microphone exists while FreeMic is running and is removed on exit.
CTL
dpkg-deb --root-owner-group -Zxz --build "$PKG" "$OUT/freemic_${VER}-1_amd64.deb" >/dev/null
rm -rf "$PKG"

# --- portable archive ----------------------------------------------------------
P=$OUT/freemic-$VER-linux-x86_64
mkdir -p "$P"
install -m755 "$BIN" "$P/freemic"
install -m644 packaging/assets/freemic-256.png "$P/freemic.png"
install -m644 README.md LICENSE THIRD-PARTY-NOTICES.md assets/fonts/Inter-LICENSE.txt "$P/"
for f in docs/*.md docs/images/*.png; do
  [ "$(basename "$f")" = WORKLOG.md ] || install -Dm644 "$f" "$P/$f"
done
cat > "$P/portable.txt" <<'TXT'
FreeMic portable

This file makes FreeMic keep its settings, soundboard and imported sounds in the
"data" folder next to the program instead of ~/.config and ~/.local/share.
Delete this file to use the normal per-user locations.

Run ./freemic. Nothing is installed: the virtual microphone is created through
your running sound server (PulseAudio, or PipeWire with pipewire-pulse) while
FreeMic runs and is removed when it exits. No root rights or drivers needed.

Needs these system libraries (present on normal desktops): glibc >= 2.35,
libpulse, libX11, libXcursor, libXi, libxkbcommon(-x11), libGL/libEGL, and
libwayland-client/-cursor/-egl on Wayland.
TXT
tar -C "$OUT" --sort=name --owner=0 --group=0 -czf "$P.tar.gz" "$(basename "$P")"
rm -rf "$P"
ls -l "$OUT"
