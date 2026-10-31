#!/bin/sh
# Copy the source from the Windows checkout into the WSL home and build it (run as tester).
set -e
SRC=/mnt/c/claude-code/FreeMic
mkdir -p ~/freemic
cd ~/freemic
for d in assets src tests tools packaging docs; do rm -rf "$d"; cp -r "$SRC/$d" .; done
cp "$SRC/Cargo.toml" "$SRC/Cargo.lock" .
[ -f "$SRC/README.md" ] && cp "$SRC/README.md" . || true
for f in LICENSE THIRD-PARTY-NOTICES.md build.rs; do [ -f "$SRC/$f" ] && cp "$SRC/$f" . || true; done
if [ -f "$HOME/.cargo/env" ]; then . "$HOME/.cargo/env"; fi
cargo build --release ${1:-} 2>&1 | grep -E "^(warning|error)|Finished|^\s+-->" | head -60
