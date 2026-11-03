#!/usr/bin/env bash
# Release notes for a tag, from CHANGELOG.md: the tag's own section (its heading's title as
# the first line, then everything up to the next version) followed by the install footer.
# Used by .github/workflows/release.yml for the draft release's description.
#
#   tools/release-notes.sh v0.12.0            # prints the notes
#   tools/release-notes.sh v0.12.0 > notes.md
set -euo pipefail

tag="${1:?usage: tools/release-notes.sh vX.Y.Z}"
version="${tag#v}"
changelog="$(dirname "$0")/../CHANGELOG.md"

section="$(awk -v v="$version" '
  /^## / {
    if (found) exit
    if (index($0, "## " v " ") == 1 || $0 == "## " v) { found = 1; next }
  }
  found { print }
' "$changelog")"

if [ -z "$section" ]; then
  echo "CHANGELOG.md has no \"## $version\" section" >&2
  exit 1
fi

# The heading's title, after the dash, in sentence case (the changelog starts it lowercase).
# awk, not sed, so it runs the same on the macOS and Windows runners.
title="$(grep -m1 "^## $version" "$changelog" | awk '{
  sub(/^## [0-9.]+[ ]*(—|-)?[ ]*/, "")
  if (length($0)) print toupper(substr($0, 1, 1)) substr($0, 2)
}')"
if [ -n "$title" ]; then
  printf '## %s\n\n' "$title"
fi

# The section without blank lines at either end.
printf '%s\n' "$section" | awk '
  { lines[NR] = $0 }
  END {
    first = 1; while (first <= NR && lines[first] ~ /^[ \t]*$/) first++
    last = NR; while (last >= first && lines[last] ~ /^[ \t]*$/) last--
    for (i = first; i <= last; i++) print lines[i]
  }'

cat <<FOOTER

---

**Install:** Mac (Apple Silicon or Intel) \`.dmg\`, Windows \`-setup.exe\`, Linux command-line tarballs (x86_64, aarch64), or the container image \`ghcr.io/casavargas/deckle\` ([docs/docker.md](https://github.com/CasaVargas/deckle/blob/$tag/docs/docker.md)). Already on 0.3 or later? Deckle offers this update itself (Settings → About).

- macOS: signed with a Developer ID and notarized by Apple.
- Windows: code-signed by Casa Vargas LLC (Azure Artifact Signing). If you used the .msi, uninstall it first.
- Kindle book conversion needs Kindle Previewer 4 (free, from Amazon) on Mac or Windows. Papers sent only by email don't. The AI on your own computer needs Ollama (free).
FOOTER
