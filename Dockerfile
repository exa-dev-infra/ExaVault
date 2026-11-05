# Build stage
FROM rust:1.80-slim as builder

WORKDIR /app
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
# Cache dependencies with a dummy main
RUN mkdir src && echo "fn main() {}" > src/main.rs && echo "" > src/lib.rs && \
    cargo build --release && \
    rm -rf src

# Copy real source code and migrations
COPY src ./src
COPY migrations ./migrations

# Touch files to force rebuild of real code
RUN touch src/main.rs src/lib.rs && cargo build --release

# Runtime stage
FROM debian:bookworm-slim

WORKDIR /app
RUN apt-get update && apt-get install -y ca-certificates libssl3 && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/template-rust /app/server
COPY migrations ./migrations
COPY .env.example ./.env

EXPOSE 8080
CMD ["/app/server"]

