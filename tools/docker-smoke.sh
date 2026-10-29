#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Smoke test for the Deckle image (Linux Docker design, "The image"): an empty /config gets a
# starter config owned by PUID:PGID, a horoscope-only paper due at 05:30 prints today's EPUB to
# /data/out on one daemon tick, and passwords come from the environment, never a keychain.
# Offline apart from the image itself.
#
#   tools/docker-smoke.sh deckle:ci
set -euo pipefail

image="${1:?usage: tools/docker-smoke.sh IMAGE}"
tmp="$(mktemp -d)"
name="deckle-smoke-$$"
mkdir "$tmp/config" "$tmp/data"

cleanup() {
    docker rm -f "$name" >/dev/null 2>&1 || true
    # The container's user owns what it wrote; remove it from inside, as root.
    docker run --rm --entrypoint rm -v "$tmp:/t" "$image" -rf /t/config /t/data >/dev/null 2>&1 || true
    rm -rf "$tmp"
}
trap cleanup EXIT

fail() {
    echo "docker-smoke: FAIL: $*" >&2
    echo "--- container log ---" >&2
    docker logs "$name" >&2 2>&1 || true
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
tick -e "DECKLE_DAEMON_FAKE_NOW=${today}T06:00:00+00:00"
epub="$tmp/data/out/deckle-daily-$today.epub"
[ -s "$epub" ] || fail "no $epub"
echo "docker-smoke: printed $(basename "$epub")"

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
