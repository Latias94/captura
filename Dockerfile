# syntax=docker/dockerfile:1

# Base toolchain with cargo-chef to cache dependencies
FROM lukemathwalker/cargo-chef:latest-rust-1.80.1 AS chef
WORKDIR /app
# Faster linking + TLS build deps for reqwest
RUN apt-get update \
 && apt-get install -y --no-install-recommends lld clang pkg-config libssl-dev \
 && rm -rf /var/lib/apt/lists/*

# Compute dependency graph
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# Build deps first (cached)
FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .
# Build only the API binary (scheduler在API进程内可开关)
RUN cargo build --release -p captura-api

# Minimal runtime image
FROM debian:bookworm-slim AS runtime
WORKDIR /app
RUN apt-get update -y \
    && apt-get install -y --no-install-recommends ca-certificates openssl curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd -m -u 10001 captura

COPY --from=builder /app/target/release/captura-api /app/captura-api

ENV RUST_LOG=info
EXPOSE 8080
USER captura
ENTRYPOINT ["/app/captura-api"]
