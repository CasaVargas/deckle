#!/bin/sh
# Fake Kindle Previewer 4 CLI for a fixed-layout book, as the real 4.0.3 behaves (fixed-layout
# spec §4): Enhanced Typesetting "Not Supported", conversion "Success", no Error row, and a joint
# KF7+KF8 MOBI under Mobi/ instead of a KPF. Copies fixtures/comic/fixed-layout-rtl.mobi.
if [ "$1" = "--help" ]; then printf 'Kindle Previewer 4.0.3 Copyright (c) Amazon.com\n'; exit 0; fi
INPUT="$1"; shift
OUT=""
while [ $# -gt 0 ]; do case "$1" in --output) OUT="$2"; shift 2;; *) shift;; esac; done
[ -d "$OUT" ] || { echo "Output should point to a valid folder."; exit 0; }
FIX="$(cd "$(dirname "$0")/../../../../fixtures/comic" && pwd)"
BASE="$(basename "$INPUT" .epub)_epub"
mkdir -p "$OUT/Mobi" "$OUT/Logs"
cp "$FIX/fixed-layout-rtl.mobi" "$OUT/Mobi/$BASE.mobi"
printf '\xEF\xBB\xBF"Type","Description"\n"Status","Book converted with warnings."\n' > "$OUT/Logs/${BASE}_log.csv"
printf '\xEF\xBB\xBF"Book Name","Enhanced Typesetting Status","Conversion Status","Error Count","Quality Issue Count","Output File Path","Log File Path","Quality Report Path"\n"%s","Not Supported","Success","0","0","%s/Mobi/%s.mobi","%s/Logs/%s_log.csv",""\n' "$BASE" "$OUT" "$BASE" "$OUT" "$BASE" > "$OUT/Summary_Log.csv"
echo "Enhanced Typesetting not supported for this book. Generating Mobi file."
