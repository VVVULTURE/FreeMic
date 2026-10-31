#!/bin/bash
# Build the Linux release artifacts inside the bookworm chroot from the Windows checkout.
# Run as root in the test VM. Usage: build_in_chroot.sh <chroot-dir> <source-dir> <out-dir>
set -eu
R=$1; SRC=$2; OUT=$3
mountpoint -q "$R/proc" || mount -t proc proc "$R/proc"
mountpoint -q "$R/dev" || { mount --rbind /dev "$R/dev"; mount --make-rslave "$R/dev"; }  # rslave: unmounting must not propagate to the host /dev
W=$R/home/tester/build
rm -rf "$W"; mkdir -p "$W"
for f in Cargo.toml Cargo.lock build.rs assets src tests tools docs packaging LICENSE README.md THIRD-PARTY-NOTICES.md; do cp -r "$SRC/$f" "$W/"; done
chroot "$R" chown -R tester:tester /home/tester/build
chroot --userspec=1000:1000 "$R" env -i HOME=/home/tester USER=tester PATH=/home/tester/.cargo/bin:/usr/bin:/bin \
  bash -c 'cd ~/build && bash packaging/linux/build.sh 2>&1 | grep -vE "^\s+(Compiling|Downloaded|Downloading)"'
umount -lR "$R/dev"; umount "$R/proc"
rm -rf "$OUT"; mkdir -p "$OUT"; cp "$W"/dist/linux/* "$OUT/"
