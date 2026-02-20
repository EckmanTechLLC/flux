# Multi-stage build for connector-manager
# Build context must be the repo root (connector-manager depends on flux crate via path = "../")
FROM rust:latest as builder

WORKDIR /app

# Copy root flux crate manifests
COPY Cargo.toml Cargo.lock ./

# Copy connector-manager crate manifests
COPY connector-manager/Cargo.toml connector-manager/Cargo.lock ./connector-manager/

# Copy source code
COPY src ./src
COPY connector-manager/src ./connector-manager/src

# Build release binary
RUN cargo build --release --manifest-path connector-manager/Cargo.toml

# Runtime stage
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/connector-manager/target/release/connector-manager /usr/local/bin/connector-manager

CMD ["connector-manager"]
