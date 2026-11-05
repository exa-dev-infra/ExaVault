.PHONY: check fmt lint test run build clean migrate-up migrate-down migrate-status migrate-create

check: fmt-check lint test
	@echo "All Rust quality checks passed successfully!"

fmt-check:
	cargo fmt --check

fmt:
	cargo fmt

lint:
	cargo clippy -- -D warnings

test:
	cargo test

run:
	cargo run

build:
	cargo build --release

# Migration & Rollback commands
migrate-up:
	cargo run --bin migrate -- up

migrate-down:
	cargo run --bin migrate -- down

migrate-status:
	cargo run --bin migrate -- status

migrate-create:
	@if [ -z "$(name)" ]; then echo "Error: Please specify migration name: make migrate-create name=migration_name"; exit 1; fi
	cargo run --bin migrate -- create $(name)

clean:
	cargo clean

