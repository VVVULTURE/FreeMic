#!/bin/bash
# makepkg (with tests) from PKGBUILD + source tarball as "tester", install with pacman,
# run the e2e test against /usr/bin/freemic, remove and check nothing is left.
# Run as root in an Arch test VM. Usage: test_pkgbuild.sh <dir with PKGBUILD and tarball>
set -u
SRC=$1; here=$(dirname "$(readlink -f "$0")")
W=/home/tester/pkgtest; rm -rf "$W"; mkdir -p "$W"; cp "$SRC/PKGBUILD" "$SRC"/freemic-*.tar.gz "$W/"; chown -R tester:tester "$W"
su - tester -c "cd $W && namcap PKGBUILD; makepkg -f > makepkg.log 2>&1; echo makepkg rc=\$?; grep -E 'test result: ok. [1-9]|FAILED|ERROR|Finished making' makepkg.log"
PKG=$(ls $W/freemic-[0-9]*.pkg.tar.zst)
namcap "$PKG" | grep -v "may not be needed"
pacman -U --noconfirm "$PKG" >/tmp/pac.log 2>&1 && echo "PASS: pacman -U" || { echo "FAIL: install"; tail /tmp/pac.log; exit 1; }
su - tester -c "cd ~ && FREEMIC=/usr/bin/freemic bash $here/e2e.sh pipewire > /tmp/e2e-pkg.out 2>&1; grep -E 'FAIL|passed' /tmp/e2e-pkg.out"
su - tester -c "rm -rf ~/.config/freemic ~/.local/share/freemic; bash $here/smoke_paths.sh /usr/bin/freemic 2>&1; find ~/.config/freemic ~/.local/share/freemic; rm -rf ~/.config/freemic ~/.local/share/freemic"
pacman -R --noconfirm freemic >/dev/null 2>&1 && echo "PASS: pacman -R"
left=$(ls -d /usr/bin/freemic /usr/share/applications/freemic.desktop /usr/share/icons/hicolor/*/apps/freemic.png /usr/share/doc/freemic /usr/share/licenses/freemic 2>/dev/null)
[ -z "$left" ] && echo "PASS: no files left after removal" || echo "FAIL: left behind: $left"
