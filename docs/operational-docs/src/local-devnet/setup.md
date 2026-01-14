# Setup

## Prerequisites

Before starting, ensure you have:

- [Rust toolchain](https://rust-lang.org/tools/install/) (use rustup for easiest setup)
- [Foundry](https://getfoundry.sh/introduction/installation/) (for compiling, testing, and deploying EVM smart contracts)
- [Docker](https://docs.docker.com/get-docker/)
- Docker Compose (usually included with Docker Desktop)
- Make (typically pre-installed on Linux/macOS; Windows users can use WSL)
- Git (for cloning the repository)

**Verify installations:**
```bash
rustc --version   # Should show rustc 1.85+
docker --version # Should show Docker 20.10+
make --version   # Should show GNU Make
```

## Installation

```bash
git clone https://github.com/informalsystems/emerald.git
cd emerald
make build
``` 

> [!NOTE]
> For building in release mode, use `make release`.