#!/bin/sh
# Fake Kindle Previewer 4 CLI that, like the real one, works in $TMPDIR/kpr_cli_<stem>_epub and
# fails when another run already holds that folder (issue #235). Otherwise behaves like ok.sh.
if [ "$1" = "--help" ]; then exec "$(dirname "$0")/ok.sh" "$@"; fi
SCRATCH="${TMPDIR:-/tmp}/kpr_cli_$(basename "$1" .epub)_epub"
mkdir "$SCRATCH" 2>/dev/null || {
  echo "[warning] [LogGenerator.cpp:89] Failed to get Mobi message stores. Log not generated"
  echo "Book Conversion failed. $1"
  exit 1
}
sleep 1
"$(dirname "$0")/ok.sh" "$@"
STATUS=$?
rmdir "$SCRATCH"
exit $STATUS
