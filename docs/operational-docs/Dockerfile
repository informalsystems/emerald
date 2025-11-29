# Use Rust official image as builder
FROM rust:1.90-slim AS builder

# Install mdbook
RUN cargo install mdbook --version 0.5.1

# Use the same base as the builder for compatibility
FROM rust:1.90-slim

# Copy mdbook binary from builder
COPY --from=builder /usr/local/cargo/bin/mdbook /usr/local/bin/mdbook

# Set working directory
WORKDIR /docs

# Copy the documentation source files
COPY book.toml .
COPY src ./src
COPY theme ./theme

# Expose the port mdbook serve uses
EXPOSE 3000

# Run mdbook serve
# --hostname 0.0.0.0 allows access from outside the container
CMD ["mdbook", "serve", "--hostname", "0.0.0.0"]
