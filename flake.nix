{
  description = "Emerald";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };

        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rust-analyzer" "clippy" "rustfmt" ];
        };
      in
      {
        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            # Rust toolchain
            rustToolchain

            # Build dependencies
            protobuf
            pkg-config
            openssl
            llvmPackages.libclang.lib
            clang

            # Foundry (Ethereum development toolkit)
            foundry

            # Testing tools
            cargo-nextest

            # Container tools (for docker compose workflow)
            docker
            docker-compose

            # Development tools
            cargo-watch
            cargo-edit
            cargo-outdated
            cargo-audit
          ];

          # Set environment variables
          RUST_BACKTRACE = "1";
          PROTOC = "${pkgs.protobuf}/bin/protoc";
          LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
        };
      }
    );
}
