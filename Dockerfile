# syntax=docker/dockerfile:1
#
# One image, one process: the server serves both the API (/api) and the
# static SvelteKit build (LEGEJO_WEB_DIR).
#
#   docker build -t legejo .
#   docker build --target check .    # tests and type checks only

FROM --platform=$BUILDPLATFORM node:24-alpine AS web
WORKDIR /web
COPY web/package.json web/package-lock.json ./
RUN --mount=type=cache,target=/root/.npm npm ci
COPY web/ ./
RUN npm run build

FROM rust:1.99-slim-bookworm AS rust
WORKDIR /src/server
ENV CARGO_INCREMENTAL=0
COPY server/ ./

FROM web AS web-check
RUN npm run check

FROM rust AS check
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/server/target \
    cargo test --locked
COPY --from=web-check /web/package.json /tmp/web-check-done

FROM rust AS server
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/server/target \
    cargo build --release --locked \
 && cp target/release/legejo /legejo \
 && cp target/release/import-sqlite /import-sqlite

FROM debian:bookworm-slim AS kepubify
ARG TARGETARCH
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl \
 && case "$TARGETARCH" in \
      amd64) asset=kepubify-linux-64bit; sum=37d7628d26c5c906f607f24b36f781f306075e7073a6fe7820a751bb60431fc5 ;; \
      arm64) asset=kepubify-linux-arm64; sum=5a15b8f6f6a96216c69330601bca29638cfee50f7bf48712795cff88ae2d03a3 ;; \
      *) echo "unsupported architecture: $TARGETARCH" >&2; exit 1 ;; \
    esac \
 && curl -fsSL -o /kepubify "https://github.com/pgaskin/kepubify/releases/download/v4.0.4/$asset" \
 && echo "$sum  /kepubify" | sha256sum -c - \
 && chmod 755 /kepubify

FROM debian:bookworm-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/*
COPY --from=kepubify /kepubify /usr/local/bin/kepubify
COPY --from=server /legejo /usr/local/bin/legejo
COPY --from=server /import-sqlite /usr/local/bin/import-sqlite
COPY --from=web /web/build /app/web
ENV LEGEJO_ADDR=0.0.0.0:3000 \
    LEGEJO_DATA_DIR=/data \
    LEGEJO_WEB_DIR=/app/web
VOLUME /data
EXPOSE 3000
HEALTHCHECK --interval=30s --timeout=5s --start-period=30s --retries=3 CMD ["legejo", "healthcheck"]
LABEL org.opencontainers.image.title="Legejo" \
      org.opencontainers.image.description="A self-hosted EPUB library with OPDS, Kobo sync and KOReader sync" \
      org.opencontainers.image.source="https://github.com/jularboolean/legejo" \
      org.opencontainers.image.licenses="AGPL-3.0-or-later"
CMD ["legejo"]
