#!/bin/sh
# Fake Previewer whose conversion fails outright (exit 0, Conversion Status Failed, no output file).
if [ "$1" = "--help" ]; then printf 'Kindle Previewer 4.0.3 Copyright (c) Amazon.com\n'; exit 0; fi
INPUT="$1"; shift
OUT=""
while [ $# -gt 0 ]; do case "$1" in --output) OUT="$2"; shift 2;; *) shift;; esac; done
[ -d "$OUT" ] || { echo "Output should point to a valid folder."; exit 0; }
BASE="$(basename "$INPUT" .epub)_epub"
mkdir -p "$OUT/Logs"
printf '\xEF\xBB\xBF"Error: This error will block the book from publishing."\n"Type","Description","Source File","Line Number","Text Preceding Warning or Error","Recommended Fix"\n"Error","E23001: The OPF file is missing a required spine element","content.opf","1","",""\n' > "$OUT/Logs/${BASE}_log.csv"
printf '\xEF\xBB\xBF"Book Name","Enhanced Typesetting Status","Conversion Status","Error Count","Quality Issue Count","Output File Path","Log File Path","Quality Report Path"\n"%s","Not Supported","Failed","1","0","","%s/Logs/%s_log.csv",""\n' "$BASE" "$OUT" "$BASE" > "$OUT/Summary_Log.csv"
echo "Conversion failed: $INPUT"
