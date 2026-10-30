#!/bin/sh
# Prepares the disposable FreeMic-Arch WSL test distro (run as root).
set -e
pacman-key --init >/dev/null 2>&1 || true
pacman-key --populate archlinux >/dev/null 2>&1 || true
pacman -Syu --noconfirm --needed base-devel git rustup pkgconf libpulse libx11 libxkbcommon libglvnd mesa wayland libxcursor libxrandr libxi \
  pipewire pipewire-pulse wireplumber xorg-server-xvfb xorg-xwininfo imagemagick dbus ffmpeg sudo >/dev/null
id tester >/dev/null 2>&1 || useradd -m -s /bin/bash tester
echo 'tester ALL=(ALL) NOPASSWD: ALL' > /etc/sudoers.d/tester
cat /etc/os-release | head -3
