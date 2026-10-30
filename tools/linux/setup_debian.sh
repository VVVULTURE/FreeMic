#!/bin/sh
# Prepares the disposable FreeMic-Debian WSL test distro (run as root).
set -e
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq build-essential pkg-config curl ca-certificates git file \
  libpulse-dev libx11-dev libxkbcommon-dev libgl-dev libwayland-dev libxcursor-dev libxrandr-dev libxi-dev \
  pulseaudio pulseaudio-utils xvfb x11-utils imagemagick dbus dbus-x11 ffmpeg dpkg-dev fakeroot >/dev/null
id tester >/dev/null 2>&1 || useradd -m -s /bin/bash tester
cat /etc/os-release | head -3
