#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Smoke test for the Deckle image (Linux Docker design, "The image"): an empty /config gets a
# starter config owned by PUID:PGID, a root `docker exec deckle` runs as PUID:PGID, a
# horoscope-only paper due at 05:30 prints today's EPUB to /data/out on one daemon tick, owned
# by PUID:PGID, and passwords come from the environment, never a keychain.
# Offline apart from the image itself.
#
#   tools/docker-smoke.sh deckle:ci
set -euo pipefail

image="${1:?usage: tools/docker-smoke.sh IMAGE}"
tmp="$(mktemp -d)"
name="deckle-smoke-$$"
mkdir "$tmp/config" "$tmp/data"

cleanup() {
    docker rm -f "$name" "$name-live" >/dev/null 2>&1 || true
    # The container's user owns what it wrote; remove it from inside, as root.
    docker run --rm --entrypoint rm -v "$tmp:/t" "$image" -rf /t/config /t/data >/dev/null 2>&1 || true
    rm -rf "$tmp"
}
trap cleanup EXIT

fail() {
    echo "docker-smoke: FAIL: $*" >&2
    echo "--- container log ---" >&2
    docker logs "$name" >&2 2>&1 || true
    docker logs "$name-live" >&2 2>&1 || true
    exit 1
}

# One daemon tick in a fresh container; extra args go before the image.
tick() {
    docker rm -f "$name" >/dev/null 2>&1 || true
    docker run --name "$name" -e DECKLE_DAEMON_ONCE=1 -e TZ=UTC \
        -v "$tmp/config:/config" -v "$tmp/data:/data" "$@" "$image" >/dev/null 2>&1 \
        || fail "daemon tick exited $?"
}

# 1. Empty volumes: a starter config, owned by the default 99:100.
tick
cfg="$tmp/config/deckle/digest.toml"
[ -f "$cfg" ] || fail "no starter digest.toml"
owner="$(stat -c %u:%g "$cfg")"
[ "$owner" = "99:100" ] || fail "digest.toml owned by $owner, want 99:100"
grep -q "wrote a starter /config/deckle/digest.toml" <<<"$(docker logs "$name" 2>&1)" \
    || fail "no starter-config log line"
echo "docker-smoke: starter config written, owned by $owner"

# 2. A horoscope-only paper due at 05:30, the daemon's clock at 06:00 today (UTC): the child
# `digest run` builds for the real date, which is the same day.
today="$(date -u +%F)"
docker run --rm -i --entrypoint sh -v "$tmp/config:/config" "$image" \
    -c 'cat > /config/deckle/digest.toml && chown 99:100 /config/deckle/digest.toml' <<'TOML'
sections = ["horoscope"]

[schedule]
at = "05:30"

[[export]]
name = "out"
dir = "/data/out"
TOML

# 2a. A root shell's `deckle` (docker exec, Unraid's console) runs as PUID:PGID: another paper
# printed that way leaves nothing root-owned, so the daemon can still take run.lock below.
docker run --rm -i --entrypoint sh -v "$tmp/config:/config" "$image" -c \
    'mkdir -p /config/deckle/papers && cat > /config/deckle/papers/extra.toml \
     && chown -R 99:100 /config/deckle/papers' <<'TOML'
title = "The Extra"
sections = ["horoscope"]

[[export]]
name = "extra"
dir = "/data/extra"
TOML
docker run -d --name "$name-live" -e TZ=UTC \
    -e "DECKLE_DAEMON_FAKE_NOW=${today}T04:00:00+00:00" \
    -v "$tmp/config:/config" -v "$tmp/data:/data" "$image" >/dev/null \
    || fail "the long-running container didn't start"
for _ in $(seq 30); do
    docker logs "$name-live" 2>&1 | grep -q "daemon: config" && break
    sleep 1
done
out="$(docker exec "$name-live" deckle digest --paper extra run 2>&1)" \
    || fail "root docker exec digest run failed: $out"
docker rm -f "$name-live" >/dev/null
# Whatever the paper's file is called: the one EPUB in its export folder.
extra="$(find "$tmp/data/extra" -name '*.epub' 2>/dev/null | head -1)"
[ -s "$extra" ] || fail "no EPUB in /data/extra from the root docker exec: $out"
# Searched as root, inside, so no folder the host user can't enter hides a root-owned file.
rooted="$(docker run --rm --entrypoint find -v "$tmp/config:/config" -v "$tmp/data:/data" \
    "$image" /config /data -uid 0)"
[ -z "$rooted" ] || fail "root docker exec left root-owned files: $rooted"
owner="$(stat -c %u:%g "$extra")"
[ "$owner" = "99:100" ] || fail "root docker exec's EPUB owned by $owner, want 99:100"
echo "docker-smoke: root docker exec ran as 99:100"

# 2b. The daemon's tick still prints, its EPUB owned by PUID:PGID.
tick -e "DECKLE_DAEMON_FAKE_NOW=${today}T06:00:00+00:00"
epub="$tmp/data/out/deckle-daily-$today.epub"
[ -s "$epub" ] || fail "no $epub"
owner="$(stat -c %u:%g "$epub")"
[ "$owner" = "99:100" ] || fail "$(basename "$epub") owned by $owner, want 99:100"
echo "docker-smoke: printed $(basename "$epub"), owned by $owner"

# 3. Passwords come only from the environment, whatever keychain libdbus might find.
docker run --rm -i --entrypoint sh -v "$tmp/config:/config" "$image" \
    -c 'cat > /config/deckle/digest.toml && chown 99:100 /config/deckle/digest.toml' <<'TOML'
sections = ["horoscope"]

[email]
enabled = true
to = "x@kindle.com"
from = "me@example.com"
smtp_host = "127.0.0.1"
smtp_port = 587
username = "me"

[[subscriptions]]
site = "gazette.example"
username = "me"
TOML
# A deckle command in the image, through the entrypoint; its output, whatever its status.
deckle_in() {
    docker run --rm -v "$tmp/config:/config" -v "$tmp/data:/data" "$@" 2>&1 || true
}
site="DECKLE_SITE_PASSWORD_GAZETTE_EXAMPLE"
out="$(deckle_in -e "$site=x" "$image" deckle --json digest subscribe)"
grep -Eq '"has_password": ?true' <<<"$out" || fail "site password not taken from $site: $out"
out="$(deckle_in "$image" deckle --json digest subscribe)"
grep -Eq '"has_password": ?false' <<<"$out" || fail "a site password without $site: $out"
out="$(deckle_in "$image" deckle digest email test)"
if ! grep -q "passwords come from the environment" <<<"$out" \
    || ! grep -q "DECKLE_SMTP_PASSWORD" <<<"$out"; then
    fail "email test without DECKLE_SMTP_PASSWORD: $out"
fi
# With it, the send gets past the password and fails on the connection (nothing on :587).
out="$(deckle_in -e DECKLE_SMTP_PASSWORD=x "$image" deckle digest email test)"
if grep -qiE "keychain|DECKLE_SMTP_PASSWORD" <<<"$out"; then
    fail "email test with DECKLE_SMTP_PASSWORD still wants a password: $out"
fi
echo "docker-smoke: passwords from the environment only"
echo "docker-smoke: OK"
