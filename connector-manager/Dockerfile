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
RUN cargo build --release -p connector-manager

# Runtime stage
FROM ubuntu:24.04

RUN apt-get update && apt-get install -y \
    ca-certificates curl \
    python3 python3-pip python3-venv \
    && curl -sSL https://github.com/warpstreamlabs/bento/releases/download/v1.15.0/bento_1.15.0_linux_amd64.tar.gz \
    | tar -xz -C /usr/local/bin bento \
    && rm -rf /var/lib/apt/lists/* \
    && apt-get clean \
    && ln -sf /usr/bin/python3 /usr/local/bin/python \
    && ln -sf /usr/bin/pip3 /usr/local/bin/pip

COPY --from=builder /app/target/release/connector-manager /usr/local/bin/connector-manager

CMD ["connector-manager"]
