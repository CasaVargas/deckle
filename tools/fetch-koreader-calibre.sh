#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Downloads KOReader's calibre wireless client (plugins/calibre.koplugin: wireless.lua,
# metadata.lua, extensions.lua), pinned and checksummed, into target/koreader-calibre/, where
# the test that runs it against Deckle's wireless server finds it
# (docs/superpowers/specs/2026-10-31-calibre-bridge-design.md §8). Needs luajit to run.
set -euo pipefail

COMMIT="7e6c9a4fb31bbcdad880945ea7e72b33b99aa7e3"
sha_for() {
  case "$1" in
    wireless.lua) echo 9f867d473fd09158f331aba49c9104f294521415db8d7cd903f556c665c188b7 ;;
    metadata.lua) echo e209612dee32ee7bf881f209b15777ffdf8a31d62a6c8d9bfe69f574e0c128d2 ;;
    extensions.lua) echo 73d3ce0590a29ff1c413086492f132fb6bf478450c2b0993a008f6027225163e ;;
  esac
}

root="$(cd "$(dirname "$0")/.." && pwd)"
dest="$root/target/koreader-calibre"
mkdir -p "$dest"
for f in wireless.lua metadata.lua extensions.lua; do
  curl -fsSL --retry 3 -o "$dest/$f.tmp" \
    "https://raw.githubusercontent.com/koreader/koreader/$COMMIT/plugins/calibre.koplugin/$f"
  got="$( (sha256sum "$dest/$f.tmp" 2>/dev/null || shasum -a 256 "$dest/$f.tmp") | cut -d' ' -f1)"
  if [ "$got" != "$(sha_for "$f")" ]; then
    echo "$f: SHA-256 $got isn't the pinned one" >&2
    exit 1
  fi
  mv "$dest/$f.tmp" "$dest/$f"
done
echo "KOReader's calibre client (koreader@${COMMIT:0:7}) in $dest"
