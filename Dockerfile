# Multi-stage build for Flux state engine
FROM rust:latest as builder

WORKDIR /app

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Copy all workspace members
COPY src ./src
COPY connector-manager ./connector-manager

# Build release binary (flux only)
RUN cargo build --release -p flux

# Runtime stage
FROM ubuntu:24.04

# Install required runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Copy binary from builder
COPY --from=builder /app/target/release/flux /usr/local/bin/flux

# Expose HTTP/WebSocket port
EXPOSE 3000

# Run Flux
CMD ["flux"]
