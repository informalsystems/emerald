# syntax=docker/dockerfile:1.5
# Build stage for emerald-simplex
FROM rust:bookworm AS builder

WORKDIR /build
ENV CARGO_HOME=/usr/local/cargo

# Install build dependencies
RUN apt-get update && apt-get install -y \
    cmake \
    libclang-dev \
    protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*

# Verify Rust version
RUN rustc --version && cargo --version

# Copy only the crates needed for emerald-simplex (excluding utils and custom-reth which have broken reth deps)
COPY Cargo.toml Cargo.lock ./

# Copy required workspace members
COPY app ./app
COPY cli ./cli
COPY engine ./engine
COPY simplex ./simplex
COPY types ./types

# Create a modified workspace Cargo.toml that excludes problematic crates
RUN sed -i 's/"utils",//' Cargo.toml && \
    sed -i '/^$/d' Cargo.toml

# Build debug binaries using build cache mounts
RUN --mount=type=cache,id=emerald-cargo-registry,target=/usr/local/cargo/registry \
    --mount=type=cache,id=emerald-cargo-git,target=/usr/local/cargo/git \
    --mount=type=cache,id=emerald-cargo-target,target=/build/target \
    CARGO_TARGET_DIR=/build/target cargo build -p emerald-simplex && \
    mkdir -p /build/bin && \
    cp /build/target/debug/emerald-simplex /build/bin/ && \
    cp /build/target/debug/emerald-simplex-setup /build/bin/

# Runtime stage
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    bash \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Copy binaries from builder
COPY --from=builder /build/bin/emerald-simplex /usr/local/bin/
COPY --from=builder /build/bin/emerald-simplex-setup /usr/local/bin/

# Create data directory
RUN mkdir -p /data /config /logs

WORKDIR /data

ENTRYPOINT ["/bin/bash", "-lc", "set -o pipefail; LOG_FILE=${SIMPLEX_LOG_FILE:-/logs/emerald-simplex.log}; LOG_DIR=$(dirname \"$LOG_FILE\"); mkdir -p \"$LOG_DIR\"; touch \"$LOG_FILE\"; emerald-simplex \"$@\" 2>&1 | tee -a \"$LOG_FILE\"", "--"]
