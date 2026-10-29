#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
# The image's entrypoint (Linux Docker design, "The image"): as root, set the zone, make the
# PUID/PGID user, own the two volumes and write a starter config; then run the command as that
# user. Started with --user, it skips straight to the command.
set -eu

log() { echo "deckle-entrypoint: $*"; }

config="${XDG_CONFIG_HOME:-/config}/deckle/digest.toml"

if [ "$(id -u)" != 0 ]; then
    [ -e "$config" ] || log "no $config; run 'deckle digest init' and edit it"
    exec "$@"
fi

PUID="${PUID:-99}"
PGID="${PGID:-100}"
case "$PUID$PGID" in
    *[!0-9]*|'') log "PUID and PGID must be numbers (got '$PUID', '$PGID')"; exit 1 ;;
esac

# One zone for every part of Deckle: the daemon reads TZ, the rest the system zone.
if [ -n "${TZ:-}" ]; then
    if [ -f "/usr/share/zoneinfo/$TZ" ]; then
        ln -sf "/usr/share/zoneinfo/$TZ" /etc/localtime
        echo "$TZ" > /etc/timezone
    else
        log "warning: TZ=$TZ is not a known zone (see /usr/share/zoneinfo); times will be off"
    fi
fi

if ! getent group "$PGID" >/dev/null; then
    if getent group deckle >/dev/null; then
        groupmod -o -g "$PGID" deckle
    else
        groupadd -g "$PGID" deckle
    fi
fi
if ! getent passwd "$PUID" >/dev/null; then
    if getent passwd deckle >/dev/null; then
        usermod -o -u "$PUID" -g "$PGID" deckle
    else
        useradd -u "$PUID" -g "$PGID" -d /data -M -s /usr/sbin/nologin deckle
    fi
fi

# Only the top level, and only when it's wrong: never a recursive chown of a user's share.
for dir in /config /data; do
    mkdir -p "$dir"
    if [ "$(stat -c %u:%g "$dir")" != "$PUID:$PGID" ]; then
        chown "$PUID:$PGID" "$dir"
    fi
done

# gosu sets HOME from the passwd entry; keep the image's.
home="$HOME"
as_user() { gosu "$PUID:$PGID" env HOME="$home" "$@"; }

if [ ! -e "$config" ]; then
    as_user deckle digest init >/dev/null
    log "wrote a starter $config — edit it, then restart"
fi

exec gosu "$PUID:$PGID" env HOME="$home" "$@"
