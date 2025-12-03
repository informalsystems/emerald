# Setup

## Prerequisites

Before starting, ensure you have:

- [Rust toolchain](https://rust-lang.org/tools/install/) (use rustup for easiest setup)
- [Foundry](https://getfoundry.sh/introduction/installation/) (for compiling, testing, and deploying EVM smart contracts)
- [Docker](https://docs.docker.com/get-docker/)
- Docker Compose (usually included with Docker Desktop)
- Make (typically pre-installed on Linux/macOS; Windows users can use WSL)
- Git (for cloning the repository)
- [Protobuf](https://protobuf.dev/installation) (to compile Emerald `.proto` files)

**Verify installations:**

```bash
rustc --version   # Should show rustc 1.88+
docker --version # Should show Docker 20.10+
make --version   # Should show GNU Make 3.81+
protoc --version # Should show libprotoc 33.1+
```

## Installation

```bash
git clone https://github.com/informalsystems/emerald.git
cd emerald
make build
```

> [!NOTE]
> For building in release mode, use `make release`.
