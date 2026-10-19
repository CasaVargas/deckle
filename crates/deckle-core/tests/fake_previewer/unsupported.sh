#!/bin/sh
# Fake Previewer that refuses Enhanced Typesetting and emits a MOBI plus an Error row.
if [ "$1" = "--help" ]; then printf 'Kindle Previewer 4.0.3 Copyright (c) Amazon.com\n'; exit 0; fi
INPUT="$1"; shift
OUT=""
while [ $# -gt 0 ]; do case "$1" in --output) OUT="$2"; shift 2;; *) shift;; esac; done
[ -d "$OUT" ] || { echo "Output should point to a valid folder."; exit 0; }
BASE="$(basename "$INPUT" .epub)_epub"
mkdir -p "$OUT/MOBI" "$OUT/Logs"
printf 'MOBI' > "$OUT/MOBI/$BASE.mobi"
printf '\xEF\xBB\xBF"Error: This error will block the book from publishing."\n"Type","Description","Source File","Line Number","Text Preceding Warning or Error","Recommended Fix"\n"Error","Fixed layout books are not supported by Enhanced Typesetting","content.opf","3","",""\n' > "$OUT/Logs/${BASE}_log.csv"
printf '\xEF\xBB\xBF"Book Name","Enhanced Typesetting Status","Conversion Status","Error Count","Quality Issue Count","Output File Path","Log File Path","Quality Report Path"\n"%s","Not Supported","Success","1","0","%s/MOBI/%s.mobi","%s/Logs/%s_log.csv",""\n' "$BASE" "$OUT" "$BASE" "$OUT" "$BASE" > "$OUT/Summary_Log.csv"
