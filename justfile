default:
    @just --list

# Full quality gate: fmt check + clippy + tests
check: fmt-check clippy test

# Run all tests
test:
    cargo test

# Run a single test by name
one name:
    cargo test {{name}}

# Check formatting
fmt-check:
    cargo fmt --check

# Fix formatting
fmt:
    cargo fmt

# Lint with warnings as errors
clippy:
    cargo clippy --all-targets -- -D warnings

# Debug build
build:
    cargo build

# Release build and install where the systemd unit ExecStart points
deploy:
    cargo build --release
    install -m 755 target/release/topmatic ~/.cargo/bin/topmatic

# Regenerate the topgrade steps fixture from the installed topgrade
fixture:
    topgrade --help > tests/fixtures/topgrade_help.txt

# Host-side dry-run verification of a profile
verify profile:
    ~/.cargo/bin/topmatic run {{profile}} --dry-run

# Host smoke: drive the real TUI in a pty (see scripts/pty_drive.py)
smoke:
    python3 scripts/pty_drive.py
