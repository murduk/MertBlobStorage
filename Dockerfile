# syntax=docker/dockerfile:1

# ---- Build stage -----------------------------------------------------------
FROM rust:1-slim-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    pkg-config \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Cache dependencies: copy only manifests first, build with dummy sources, then
# copy real sources and rebuild — avoids re-downloading/compiling every crate
# on every source change.
COPY Cargo.toml Cargo.lock ./
COPY crates/blobstore-core/Cargo.toml crates/blobstore-core/Cargo.toml
COPY crates/blobstore-storage/Cargo.toml crates/blobstore-storage/Cargo.toml
COPY crates/blobstore-auth/Cargo.toml crates/blobstore-auth/Cargo.toml
COPY crates/blobstore-xml/Cargo.toml crates/blobstore-xml/Cargo.toml
COPY crates/blobstore-api/Cargo.toml crates/blobstore-api/Cargo.toml
COPY crates/blobstore-server/Cargo.toml crates/blobstore-server/Cargo.toml

RUN for crate in blobstore-core blobstore-storage blobstore-auth blobstore-xml blobstore-api; do \
        mkdir -p crates/$crate/src && echo "// placeholder" > crates/$crate/src/lib.rs; \
    done && \
    mkdir -p crates/blobstore-server/src && echo "fn main() {}" > crates/blobstore-server/src/main.rs

RUN cargo build --release --package blobstore-server || true

# Now bring in the real sources and migrations, and do the real build.
COPY crates ./crates
RUN touch crates/*/src/lib.rs crates/blobstore-server/src/main.rs && \
    cargo build --release --package blobstore-server

# ---- Runtime stage ----------------------------------------------------------
FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    wget \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --create-home --home-dir /app --shell /usr/sbin/nologin mertblobstorage

WORKDIR /app

COPY --from=builder /build/target/release/mertblobstorage /usr/local/bin/mertblobstorage
COPY config/default.toml /app/config/default.toml

RUN mkdir -p /app/data && chown -R mertblobstorage:mertblobstorage /app

USER mertblobstorage
VOLUME ["/app/data"]

EXPOSE 8443

HEALTHCHECK --interval=30s --timeout=3s --start-period=5s \
    CMD wget -qO- http://127.0.0.1:8443/healthz || exit 1

ENTRYPOINT ["mertblobstorage"]
CMD ["--config", "/app/config/default.toml"]
