#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Downloads the pinned PDFium build for this machine into target/pdfium/, where a development
# build of Deckle (and `cargo test`) finds it. A release build looks beside the executable or in
# DECKLE_PDFIUM instead (docs/superpowers/specs/2026-10-23-pdf-reflow-design.md §3).
#
# The checked archive and what it unpacks to are kept once per machine, in
# $DECKLE_PDFIUM_CACHE or the user cache (~/Library/Caches/deckle/pdfium on macOS,
# ~/.cache/deckle/pdfium elsewhere, %LOCALAPPDATA%\deckle\pdfium on Windows). A development
# build looks there too, so every checkout and worktree finds it after one run anywhere, and a
# second run downloads nothing.
#
#   tools/fetch-pdfium.sh            # this machine
#   tools/fetch-pdfium.sh mac-arm64  # a named platform: linux-x64, linux-arm64, mac-arm64, mac-x64, win-x64
set -euo pipefail

TAG="chromium/8111"
sha_for() {
  case "$1" in
    linux-x64) echo bdc60126399f39fe768566a18d4be055a7b3681bc9e248e8a6656b4f6cae0aa2 ;;
    linux-arm64) echo 042de247138f96341534cccf24ed21c2550a7245b936bb4a60d2340120000f05 ;;
    mac-arm64) echo 46c674298c60123f1195c2a03c65ee08c4783c84874ce4f5b3a054abb3ecaf30 ;;
    mac-x64) echo e7fece4693f65a7e47f84fa38099068d573d4dcbd46b1a3c8d2c26581c308466 ;;
    win-x64) echo 3452d11031050ec65e57f180215b57b9e6d465e3fbedafa8a77ad90885c7d23e ;;
    *) echo "unknown platform $1" >&2; exit 2 ;;
  esac
}

platform="${1:-}"
if [ -z "$platform" ]; then
  case "$(uname -s)-$(uname -m)" in
    Linux-x86_64) platform=linux-x64 ;;
    Linux-aarch64) platform=linux-arm64 ;;
    Darwin-arm64) platform=mac-arm64 ;;
    Darwin-x86_64) platform=mac-x64 ;;
    MINGW*|MSYS*|CYGWIN*) platform=win-x64 ;;
    *) echo "no pinned PDFium for $(uname -s) $(uname -m)" >&2; exit 2 ;;
  esac
fi

root="$(cd "$(dirname "$0")/.." && pwd)"
dest="$root/target/pdfium"

# Must match crates/deckle-pdf/src/engine.rs `cache_dir` (a test there checks the tag).
if [ -n "${DECKLE_PDFIUM_CACHE:-}" ]; then
  cache="$DECKLE_PDFIUM_CACHE"
elif [ "$(uname -s)" = Darwin ]; then
  cache="$HOME/Library/Caches/deckle/pdfium"
elif [ -n "${LOCALAPPDATA:-}" ]; then
  cache="$LOCALAPPDATA/deckle/pdfium"
else
  cache="${XDG_CACHE_HOME:-$HOME/.cache}/deckle/pdfium"
fi
here="$cache/${TAG//\//-}/$platform"
archive="$here/pdfium.tgz"
want="$(sha_for "$platform")"

# Hashed from stdin: given a name with a backslash in it (a Windows cache path), sha256sum
# puts a backslash in front of the hash.
sha_of() {
  if command -v sha256sum >/dev/null; then
    sha256sum <"$1" | cut -d' ' -f1
  else
    shasum -a 256 <"$1" | cut -d' ' -f1
  fi
}

if [ ! -f "$archive" ] || [ "$(sha_of "$archive")" != "$want" ]; then
  mkdir -p "$here"
  part="$archive.part"
  curl -fsSL --retry 3 -o "$part" \
    "https://github.com/bblanchon/pdfium-binaries/releases/download/$TAG/pdfium-$platform.tgz"
  got="$(sha_of "$part")"
  if [ "$got" != "$want" ]; then
    rm -f "$part"
    echo "PDFium archive checksum mismatch: got $got, want $want" >&2
    exit 1
  fi
  mv "$part" "$archive"
  rm -rf "$here/lib" "$here/bin" "$here/include"
  tar xzf "$archive" -C "$here"
fi
rm -rf "$dest"
mkdir -p "$dest"
tar xzf "$archive" -C "$dest"
echo "PDFium ($TAG, $platform) in $dest (kept in $here)"
