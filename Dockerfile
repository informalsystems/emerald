FROM rust:1.90.0-bookworm AS build-env

ARG TAG

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

FROM debian:trixie-slim

RUN apt-get update -qq && \
	apt-get install -yqq --no-install-recommends \
		libssl-dev \
		ca-certificates && \
	apt-get clean && \
	rm -rf /var/lib/apt/lists/*

COPY --chown=0:0 --from=build-env /root/target/release/emerald /usr/local/bin/emerald
COPY --chown=0:0 --from=build-env /root/target/release/emerald-utils /usr/local/bin/emerald-utils

RUN useradd -m emerald -s /bin/bash
WORKDIR /home/emerald
USER emerald:emerald

ENTRYPOINT ["/usr/local/bin/emerald"]
