#!/bin/sh
# Fake Previewer that succeeds but logs a "media file not found" notice.
# Usage mirrors the real CLI: <input> --convert --locale en --output <dir>
if [ "$1" = "--help" ]; then
  printf '**************************************************\n\nKindle Previewer 4.0.3 Copyright (c) Amazon.com\n\n**************************************************\n'
  exit 0
fi
INPUT="$1"; shift
OUT=""
while [ $# -gt 0 ]; do
  case "$1" in
    --output) OUT="$2"; shift 2;;
    *) shift;;
  esac
done
[ -d "$OUT" ] || { echo "Output should point to a valid folder."; exit 0; }
FIX="$(cd "$(dirname "$0")/../../../../fixtures/mini" && pwd)"
BASE="$(basename "$INPUT" .epub)_epub"
mkdir -p "$OUT/KPF" "$OUT/Logs"
cp "$FIX/deckle-mini.kpf" "$OUT/KPF/$BASE.kpf"
cp "$FIX/deckle-mini_epub_log.csv" "$OUT/Logs/${BASE}_log.csv"
printf '"Notice","W14010: media file not found Images/cover.png","cover.xhtml","","",""\n' >> "$OUT/Logs/${BASE}_log.csv"
printf '\xEF\xBB\xBF"Book Name","Enhanced Typesetting Status","Conversion Status","Error Count","Quality Issue Count","Output File Path","Log File Path","Quality Report Path"\n"%s","Supported","Success","0","0","%s/KPF/%s.kpf","%s/Logs/%s_log.csv",""\n' "$BASE" "$OUT" "$BASE" "$OUT" "$BASE" > "$OUT/Summary_Log.csv"
echo "Book converted successfully! : $INPUT"
