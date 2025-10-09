FROM rust:1.85.0-bookworm AS build-env
ARG TAG
WORKDIR /root
RUN apt-get update && apt-get install -y \
	build-essential \
	ca-certificates \
	git pkg-config libssl-dev clang libclang-dev llvm-dev protobuf-compiler \
	&& rm -rf /var/lib/apt/lists/*

COPY . /root
RUN cargo build --release

FROM debian:bookworm-slim

RUN useradd -m mbft -s /bin/bash && apt-get update && apt-get install -y libssl-dev ca-certificates && apt-get clean
WORKDIR /home/mbft
USER mbft:mbft

COPY --chown=0:0 --from=build-env /root/target/release/malachitebft-eth-app /usr/local/bin/malachitebft-eth-app
COPY --chown=0:0 --from=build-env /root/target/release/malachitebft-eth-utils /usr/local/bin/malachitebft-eth-utils

VOLUME ["/home/mbft/.malachite"]

RUN mkdir -p /home/mbft/.malaketh/config
RUN mkdir -p /home/mbft/.malachite

ENTRYPOINT ["/usr/local/bin/malachitebft-eth-app"]
