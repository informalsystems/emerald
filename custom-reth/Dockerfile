FROM rustlang/rust:nightly-trixie-slim AS builder

RUN apt-get update && apt-get install -y \
    build-essential \
    clang \
    cmake \
    libclang-dev \
    && rm -rf /var/lib/apt/lists/*


WORKDIR /build

COPY Cargo.toml ./
COPY src ./src

RUN ls -la

RUN cargo build --release --bin custom-reth

FROM debian:trixie-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/custom-reth /usr/local/bin

ENTRYPOINT ["/usr/local/bin/custom-reth"]
