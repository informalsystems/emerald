FROM rust:1.90.0-bookworm AS build-env

ARG TAG

ENV DEBIAN_FRONTEND=noninteractive
ENV CARGO_TERM_COLOR=never
ENV CARGO_TERM_PROGRESS_WHEN=never

WORKDIR /root

RUN apt-get update -qq && \
	apt-get install -yqq --no-install-recommends \
	build-essential \
	ca-certificates \
	clang \
	git \
	libclang-dev \
	libssl-dev \
	llvm-dev \
	pkg-config \
	protobuf-compiler && \
	rm -rf /var/lib/apt/lists/*

COPY . /root
RUN cargo build --release --locked

FROM debian:bookworm-slim

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update -qq && \
	apt-get install -yqq --no-install-recommends \
	libssl-dev \
	ca-certificates && \
	apt-get clean && \
	rm -rf /var/lib/apt/lists/*

COPY --chown=0:0 --from=build-env /root/target/release/malachitebft-eth-app /usr/local/bin/malachitebft-eth-app
COPY --chown=0:0 --from=build-env /root/target/release/malachitebft-eth-utils /usr/local/bin/malachitebft-eth-utils

RUN useradd -m emerald -s /bin/bash
WORKDIR /home/emerald
USER emerald:emerald


RUN mkdir -p /home/emerald/.malaketh/config /home/emerald/.emerald

ENTRYPOINT ["/usr/local/bin/malachitebft-eth-app"]
