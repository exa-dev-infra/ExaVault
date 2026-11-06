# Build stage
FROM rust:1-bookworm as builder

WORKDIR /app
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./

# Cache dependencies with a dummy main and lib
RUN mkdir src && \
    echo "fn main() {}" > src/main.rs && \
    echo "" > src/lib.rs && \
    cargo build --release && \
    rm -rf src

# Copy real source code, templates, public assets and migrations
COPY src ./src
COPY templates ./templates
COPY public ./public
COPY migrations ./migrations

# Remove dummy build artifacts for exavault so cargo rebuilds the real crate cleanly
RUN rm -f target/release/deps/exavault* target/release/exavault* && \
    cargo build --release

# Runtime stage
FROM debian:bookworm-slim

WORKDIR /app
RUN apt-get update && apt-get install -y ca-certificates libssl3 && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/exavault /app/server
COPY --from=builder /app/target/release/migrate /app/migrate
COPY migrations ./migrations
COPY .env.example ./.env

EXPOSE 8080
CMD ["/app/server", "serve"]

