#!/bin/bash
# Run the portable-archive test inside a Debian chroot (e.g. bookworm made with
# debootstrap) to check compatibility with an older glibc. Run as root in a test VM.
# Usage: chroot_test.sh <chroot-dir> <archive.tar.gz> <fmtest-binary>
set -eu
R=$1; ARCHIVE=$2; FMTEST=$3
here=$(dirname "$(readlink -f "$0")")
mountpoint -q "$R/proc" || mount -t proc proc "$R/proc"
mountpoint -q "$R/dev" || { mount --rbind /dev "$R/dev"; mount --make-rslave "$R/dev"; }  # rslave: unmounting must not propagate to the host /dev
chroot "$R" id tester >/dev/null 2>&1 || chroot "$R" useradd -m -u 1000 -s /bin/bash tester
H=$R/home/tester
mkdir -p "$H/freemic/target/release" "$H/freemic/tools/linux"
cp "$FMTEST" "$H/freemic/target/release/fmtest"
cp "$here"/e2e.sh "$here"/smoke_paths.sh "$here"/portable_test.sh "$H/freemic/tools/linux/"
cp "$ARCHIVE" "$H/portable.tar.gz"
chroot "$R" chown -R tester:tester /home/tester
chroot "$R" ldd --version | head -1
chroot --userspec=1000:1000 "$R" env -i HOME=/home/tester USER=tester PATH=/usr/bin:/bin \
  bash /home/tester/freemic/tools/linux/portable_test.sh /home/tester/portable.tar.gz pulseaudio
umount -lR "$R/dev" || true
umount "$R/proc" || true
