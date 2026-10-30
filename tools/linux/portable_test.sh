#!/bin/bash
# Extract the portable archive, check it keeps data next to itself, run the e2e test on it.
# Usage: portable_test.sh <archive.tar.gz> pulseaudio|pipewire
set -u
ARCHIVE=$1; SERVER=$2
D=$(mktemp -d /tmp/fm-port-XXXX)
tar -C "$D" -xzf "$ARCHIVE"
APP=$(echo "$D"/freemic-*/freemic)
rm -rf ~/.config/freemic ~/.local/share/freemic
bash "$(dirname "$0")/smoke_paths.sh" "$APP"
echo "files in portable folder:"; (cd "$(dirname "$APP")" && find . | sort)
[ -e ~/.config/freemic ] || [ -e ~/.local/share/freemic ] && echo "FAIL: wrote to home" || echo "PASS: nothing written to ~/.config or ~/.local/share"
grep -m1 starting "$(dirname "$APP")/data/freemic.log"
FREEMIC=$APP bash "$(dirname "$0")/e2e.sh" "$SERVER" > /tmp/e2e-portable.out 2>&1
grep -E "FAIL|passed" /tmp/e2e-portable.out
rm -rf "$D"
