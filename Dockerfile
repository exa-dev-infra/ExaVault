# Build stage
FROM rust:1-bookworm as builder

WORKDIR /app
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

# Copy workspace sources and assets
COPY . .

# Build production binary
RUN cargo build --release --bin exavault

# Runtime stage
FROM debian:bookworm-slim

WORKDIR /app
RUN apt-get update && apt-get install -y ca-certificates libssl3 && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/exavault /app/server
COPY migrations ./migrations
COPY .env.example ./.env

EXPOSE 8080
CMD ["/app/server", "serve"]
