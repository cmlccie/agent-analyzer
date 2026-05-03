.PHONY: all fmt lint test build release check clean mock status

all: fmt lint test build

fmt:
	cargo fmt

lint:
	cargo fmt --check
	cargo clippy --all-targets --all-features -- -D warnings

test:
	cargo test

build:
	cargo build

release:
	cargo build --release

check: lint test

clean:
	cargo clean

# Run the mock server on localhost:8000 (Ctrl-C to stop)
mock:
	cargo run -- serve mock

# Point status at the mock server (requires mock to be running)
status:
	cargo run -- status
