FROM rustlang/rust:nightly-bookworm AS builder

RUN apt-get update && apt-get install -y \
    build-essential \
    clang \
    cmake \
    libclang-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

COPY custom-reth ./custom-reth

WORKDIR /build/custom-reth
RUN cargo build --release --bin custom-reth

FROM ubuntu:22.04

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/custom-reth/target/release/custom-reth /usr/local/bin/custom-reth

ENTRYPOINT ["custom-reth"]

CMD ["node"]
