# syntax=docker/dockerfile:1
# SPDX-License-Identifier: GPL-3.0-or-later
# Deckle, headless: `deckle serve` on Linux, the web UI on 8080 and the `digest daemon` schedule
# (docs/superpowers/specs/2026-10-29-linux-docker-design.md, "The image"). Built from the
# repository root: docker build -f docker/Dockerfile .

# The web UI: the desktop app's frontend, built for the browser. pnpm pinned as in CI. The
# output is plain files, so it's built once on the build machine's own architecture.
FROM --platform=$BUILDPLATFORM node:22-bookworm-slim AS web
ENV COREPACK_ENABLE_DOWNLOAD_PROMPT=0
RUN corepack enable && corepack prepare pnpm@12.5.1 --activate
WORKDIR /src/apps/desktop
COPY apps/desktop/package.json apps/desktop/pnpm-lock.yaml apps/desktop/pnpm-workspace.yaml ./
RUN pnpm install --frozen-lockfile
COPY apps/desktop/index.html apps/desktop/tsconfig.json apps/desktop/vite.config.ts ./
COPY apps/desktop/src ./src
# The masthead faces the frontend imports (vite.config.ts allows the folder).
COPY crates/deckle-digest/fonts /src/crates/deckle-digest/fonts
RUN pnpm build:web

# rust-toolchain.toml says stable; rust:1 tracks it.
FROM rust:1-bookworm AS build
ARG TARGETARCH
# keyring's Secret Service backend links libdbus (unused in the container, still linked).
RUN apt-get update \
 && apt-get install -y --no-install-recommends libdbus-1-dev pkg-config \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY . .
# `deckle-web`: the `deckle` command line plus `serve`, installed as `deckle` (the daemon's
# children re-exec it). The workspace manifest still loads every member, the Tauri app included.
# Cache mounts speed up a local rebuild; the binary is copied out of the cached target.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target,sharing=locked \
    cargo build --release -p deckle-server --bin deckle-web --locked \
 && mkdir -p /out \
 && cp target/release/deckle-web /out/deckle
# The pinned, checksummed PDFium for this architecture (PDF reflow design §3).
RUN case "$TARGETARCH" in \
      amd64) platform=linux-x64 ;; \
      arm64) platform=linux-arm64 ;; \
      *) echo "no pinned PDFium for $TARGETARCH" >&2; exit 1 ;; \
    esac \
 && tools/fetch-pdfium.sh "$platform" \
 && cp target/pdfium/lib/libpdfium.so /out/libpdfium.so

FROM debian:bookworm-slim
# No Tauri/WebKit: the browser is the UI. Fonts are bundled in the binary; DejaVu covers anything
# rendered outside it. tzdata so TZ names resolve; gosu to drop root after setup; tini as PID 1
# so signals reach the daemon and its exited children are reaped.
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates tzdata gosu tini fonts-dejavu-core libdbus-1-3 \
 && rm -rf /var/lib/apt/lists/*
COPY --from=build /out/deckle /out/libpdfium.so /opt/deckle/
COPY --from=web /src/apps/desktop/dist-web /opt/deckle/web
COPY LICENSE /usr/share/doc/deckle/LICENSE
COPY --chmod=0755 docker/entrypoint.sh /usr/local/bin/entrypoint.sh
# On PATH, a wrapper: a root `docker exec deckle deckle …` runs as PUID:PGID.
COPY --chmod=0755 docker/deckle.sh /usr/local/bin/deckle

LABEL org.opencontainers.image.title="Deckle" \
      org.opencontainers.image.source="https://github.com/CasaVargas/deckle" \
      org.opencontainers.image.licenses="GPL-3.0-or-later"

ENV PUID=99 \
    PGID=100 \
    TZ=UTC \
    XDG_CONFIG_HOME=/config \
    HOME=/data \
    DECKLE_DAEMON=1 \
    DECKLE_BOOKMARKLET_BIND=0.0.0.0 \
    DECKLE_PDFIUM=/opt/deckle/libpdfium.so \
    DECKLE_WEB_BIND=0.0.0.0 \
    DECKLE_WEB_DIR=/opt/deckle/web
VOLUME /config /data
# The web UI (off until DECKLE_WEB_PASSWORD is set), and the bookmarklet listener.
EXPOSE 8080 47219 47220
ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/entrypoint.sh"]
# The web UI and the daemon; without a password, the daemon alone.
CMD ["deckle", "serve"]
