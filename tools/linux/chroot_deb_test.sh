#!/bin/bash
# Install the .deb inside a Debian chroot (e.g. bookworm), run the e2e test against
# /usr/bin/freemic, then remove it. Run as root in a test VM.
# Usage: chroot_deb_test.sh <chroot-dir> <package.deb> <fmtest-binary>
set -u
R=$1; DEB=$2; FMTEST=$3
here=$(dirname "$(readlink -f "$0")")
mountpoint -q "$R/proc" || mount -t proc proc "$R/proc"
mountpoint -q "$R/dev" || { mount --rbind /dev "$R/dev"; mount --make-rslave "$R/dev"; }
chroot "$R" id tester >/dev/null 2>&1 || chroot "$R" useradd -m -u 1000 -s /bin/bash tester
mkdir -p "$R/home/tester/freemic/target/release" "$R/home/tester/freemic/tools/linux"
cp "$FMTEST" "$R/home/tester/freemic/target/release/fmtest"
cp "$here"/e2e.sh "$R/home/tester/freemic/tools/linux/"
cp "$DEB" "$R/tmp/freemic.deb"
chroot "$R" chown -R tester:tester /home/tester
chroot "$R" apt-get install -y -qq xdotool >/dev/null 2>&1
chroot "$R" apt-get install -y /tmp/freemic.deb >/tmp/chroot-deb.log 2>&1 && echo "PASS: apt-get install on $(chroot "$R" cat /etc/debian_version)" || { echo "FAIL: install"; tail -5 /tmp/chroot-deb.log; }
chroot --userspec=1000:1000 "$R" env -i HOME=/home/tester USER=tester PATH=/usr/bin:/bin \
  bash -c 'cd ~ && FREEMIC=/usr/bin/freemic bash freemic/tools/linux/e2e.sh pulseaudio > /tmp/e2e-chroot-deb.out 2>&1; grep -E "FAIL|passed" /tmp/e2e-chroot-deb.out'
chroot "$R" apt-get remove -y -qq freemic >/dev/null 2>&1 && echo "PASS: apt-get remove"
[ -e "$R/usr/bin/freemic" ] && echo "FAIL: binary left" || echo "PASS: removed"
umount -lR "$R/dev"; umount "$R/proc"
