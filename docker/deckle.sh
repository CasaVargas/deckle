#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
# `deckle` in the image (Linux Docker design, "The image"). `docker exec` and Unraid's console
# run as root; a root `digest run` would leave root-owned files (run.lock, work folders) under
# /data that the daemon, as PUID:PGID, can never take again. So root runs as that user too.
if [ "$(id -u)" = 0 ] && [ -n "${DECKLE_DAEMON:-}" ]; then
    exec gosu "${PUID:-99}:${PGID:-100}" env HOME="$HOME" /opt/deckle/deckle "$@"
fi
exec /opt/deckle/deckle "$@"
