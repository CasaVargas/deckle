#!/usr/bin/env python3
"""Checks every pin on the KOReader plugin store's Recommended list
(crates/deckle-core/koplugin/recommended.json, KOReader plugins design part C): the release asset
still downloads, its SHA-256 is the pinned one, and it holds exactly one .koplugin with a
main.lua. Exits 1 when any pin fails, so a deleted or replaced release shows up."""
import hashlib
import io
import json
import sys
import urllib.request
import zipfile

LIST = "crates/deckle-core/koplugin/recommended.json"


def check(p):
    r = p["release"]
    url = f"https://github.com/{p['repo']}/releases/download/{r['tag']}/{r['asset']}"
    with urllib.request.urlopen(urllib.request.Request(url, headers={"User-Agent": "Deckle CI"}), timeout=60) as resp:
        data = resp.read()
    got = hashlib.sha256(data).hexdigest()
    if got != r["sha256"].lower():
        return f"SHA-256 is {got}, pinned {r['sha256']}"
    names = zipfile.ZipFile(io.BytesIO(data)).namelist()
    roots = {n.split("/")[i] for n in names for i, part in enumerate(n.split("/")) if part.endswith(".koplugin")}
    if len(roots) != 1:
        return f"holds {len(roots)} .koplugin folders"
    if not any(n.endswith(".koplugin/main.lua") for n in names):
        return "no main.lua"
    if next(iter(roots)) != p["name"] + ".koplugin":
        return f"holds {next(iter(roots))}, not {p['name']}.koplugin"
    return None


def main():
    with open(LIST) as f:
        rec = json.load(f)
    failed = 0
    for p in rec["plugins"]:
        try:
            why = check(p)
        except Exception as e:  # a network error is a failed pin too
            why = str(e)
        print(f"{'FAIL' if why else 'ok  '} {p['repo']} {p['release']['tag']}{': ' + why if why else ''}")
        failed += bool(why)
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
