# syntax=docker/dockerfile:1
# SPDX-License-Identifier: GPL-3.0-or-later
# Deckle, headless: `deckle digest daemon` on Linux (docs/superpowers/specs/2026-10-29-linux-docker-design.md,
# "The image"). Built from the repository root: docker build -f docker/Dockerfile .

# rust-toolchain.toml says stable; rust:1 tracks it.
FROM rust:1-bookworm AS build
ARG TARGETARCH
# keyring's Secret Service backend links libdbus (unused in the container, still linked).
RUN apt-get update \
 && apt-get install -y --no-install-recommends libdbus-1-dev pkg-config \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY . .
# Only the CLI; the workspace manifest still loads every member, the Tauri app included.
# Cache mounts speed up a local rebuild; the binary is copied out of the cached target.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target,sharing=locked \
    cargo build --release -p deckle-cli --locked \
 && mkdir -p /out \
 && cp target/release/deckle /out/deckle
# The pinned, checksummed PDFium for this architecture (PDF reflow design §3).
RUN case "$TARGETARCH" in \
      amd64) platform=linux-x64 ;; \
      arm64) platform=linux-arm64 ;; \
      *) echo "no pinned PDFium for $TARGETARCH" >&2; exit 1 ;; \
    esac \
 && tools/fetch-pdfium.sh "$platform" \
 && cp target/pdfium/lib/libpdfium.so /out/libpdfium.so

FROM debian:bookworm-slim
# No Tauri/WebKit: the CLI only. Fonts are bundled in the binary; DejaVu covers anything
# rendered outside it. tzdata so TZ names resolve; gosu to drop root after setup.
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates tzdata gosu fonts-dejavu-core libdbus-1-3 \
 && rm -rf /var/lib/apt/lists/*
COPY --from=build /out/deckle /out/libpdfium.so /opt/deckle/
COPY LICENSE /usr/share/doc/deckle/LICENSE
COPY --chmod=0755 docker/entrypoint.sh /usr/local/bin/entrypoint.sh
RUN ln -s /opt/deckle/deckle /usr/local/bin/deckle

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
    DECKLE_PDFIUM=/opt/deckle/libpdfium.so
VOLUME /config /data
# The bookmarklet listener.
EXPOSE 47219
ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]
CMD ["deckle", "digest", "daemon"]
