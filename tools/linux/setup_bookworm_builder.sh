#!/bin/bash
# Turn a Debian 12 chroot (debootstrap --variant=minbase bookworm) into the release
# build environment: glibc 2.36, so the binary runs on Debian 12+, Ubuntu 22.04+ (glibc
# >= 2.35) and current Arch. Run as root in a test VM. Usage: setup_bookworm_builder.sh <dir>
set -eu
R=$1
mountpoint -q "$R/proc" || mount -t proc proc "$R/proc"
mountpoint -q "$R/dev" || { mount --rbind /dev "$R/dev"; mount --make-rslave "$R/dev"; }  # rslave: unmounting must not propagate to the host /dev
chroot "$R" id tester >/dev/null 2>&1 || chroot "$R" useradd -m -u 1000 -s /bin/bash tester
chroot "$R" apt-get install -y -qq build-essential pkg-config libpulse-dev curl ca-certificates xz-utils >/tmp/apt.log 2>&1
chroot --userspec=1000:1000 "$R" env -i HOME=/home/tester USER=tester PATH=/usr/bin:/bin bash -c \
  'curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain 1.99.1 >/tmp/rustup.log 2>&1; ~/.cargo/bin/cargo --version'
umount -lR "$R/dev"; umount "$R/proc"
