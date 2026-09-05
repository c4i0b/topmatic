.PHONY: check fmt fmt-check clippy test build

check: fmt-check clippy test

fmt:
	cargo fmt

fmt-check:
	cargo fmt --check

clippy:
	cargo clippy --all-targets -- -D warnings

test:
	cargo test

build:
	cargo build
