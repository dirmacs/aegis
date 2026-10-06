# syntax=docker/dockerfile:1.7
#
# aegis — dirmacs system configuration manager (CLI).
#
# Multi-stage build: compiles the `aegis` binary, then ships it in a slim
# runtime as a non-root user. `aegis` is a host-mutation CLI (no long-running
# server), so the image is a build/distribution vehicle: run it with the host
# config tree mounted at /work.
#
# Build:  docker build -t aegis .
# Run:    docker run --rm -v "$PWD:/work" aegis status
#         docker run --rm -v "$PWD:/work" aegis sync --dry-run
#
# Note: many subcommands (sync/enforce/bootstrap/net) mutate host state and
# need extra privileges/mounts; run those on the host, not in this container.

# ---- builder ---------------------------------------------------------------
FROM rust:1.98-bookworm AS builder

WORKDIR /app

# Cargo.lock IS tracked in this repo → copy it and build --locked.
COPY Cargo.toml Cargo.lock ./
COPY crates/ ./crates/

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --release --locked --bin aegis && \
    cp /app/target/release/aegis /tmp/aegis

# ---- runtime ---------------------------------------------------------------
FROM debian:bookworm-slim AS runtime

RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates git && \
    rm -rf /var/lib/apt/lists/* && \
    useradd --create-home --uid 1000 --shell /usr/sbin/nologin aegis

COPY --from=builder /tmp/aegis /usr/local/bin/aegis

RUN mkdir -p /work && chown -R aegis:aegis /work
WORKDIR /work

USER aegis

ENV RUST_LOG=info

# CLI: no EXPOSE (nothing listens). ENTRYPOINT so args pass straight through.
ENTRYPOINT ["aegis"]
CMD ["--help"]
