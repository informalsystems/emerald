# Installation

## Prerequisites 

- [Rust toolchain](https://rust-lang.org/tools/install/)

## Installing Emerald

```
git clone https://github.com/informalsystems/emerald.git
cd emerald
cargo build --release
```

This will build the Emerald binary and place it under `target/release/emerald` which can then be copied to the desired machine under `/usr/local/bin/emerald` for example.

## Installing Reth

```
git clone https://github.com/informalsystems/emerald.git
cd emerald/custom-reth
cargo build --release
```

This will build the Reth binary and place it under `target/release/custom-reth` which can then be copied to the desired machine under `/usr/local/bin/custom-reth` for example.