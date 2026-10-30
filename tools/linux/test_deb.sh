#!/bin/bash
# Install the .deb, run the e2e test against /usr/bin/freemic as user "tester",
# check the default data paths, then remove the package and check nothing is left.
# Run as root in a Debian/Ubuntu test VM. Usage: test_deb.sh <package.deb>
set -u
DEB=$1; here=$(dirname "$(readlink -f "$0")")
apt-get install -y -qq "$DEB" >/tmp/deb-install.log 2>&1 && echo "PASS: apt-get install" || { echo "FAIL: install"; tail /tmp/deb-install.log; exit 1; }
dpkg -s freemic | grep -E '^(Status|Version)'
desktop-file-validate /usr/share/applications/freemic.desktop && echo "PASS: desktop file valid"
su - tester -c "cd ~ && FREEMIC=/usr/bin/freemic bash $here/e2e.sh pulseaudio > /tmp/e2e-deb.out 2>&1; grep -E 'FAIL|passed' /tmp/e2e-deb.out"
su - tester -c "rm -rf ~/.config/freemic ~/.local/share/freemic; bash $here/smoke_paths.sh /usr/bin/freemic 2>&1; find ~/.config/freemic ~/.local/share/freemic; rm -rf ~/.config/freemic ~/.local/share/freemic"
apt-get remove -y -qq freemic >/dev/null 2>&1 && echo "PASS: apt-get remove"
left=$(ls -d /usr/bin/freemic /usr/share/applications/freemic.desktop /usr/share/icons/hicolor/*/apps/freemic.png /usr/share/doc/freemic 2>/dev/null)
[ -z "$left" ] && echo "PASS: no files left after removal" || echo "FAIL: left behind: $left"
