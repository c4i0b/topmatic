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

# Build the devcontainer image
image:
    podman build -t topmatic-dev .devcontainer/

# Interactive shell inside the devcontainer (volume mounts mirror container-gate)
shell: image
    podman run --rm -it --userns=keep-id \
        -v topmatic-cargo:/usr/local/cargo \
        -v "$PWD:/workspace" -w /workspace topmatic-dev

# Quality gate inside the devcontainer image (deps cache persists in topmatic-cargo volume)
container-gate: image
    podman run --rm --userns=keep-id \
        -v topmatic-cargo:/usr/local/cargo \
        -v "$PWD:/workspace" -w /workspace topmatic-dev just check

# Host-side dry-run verification of a profile
verify profile:
    ~/.cargo/bin/topmatic run {{profile}} --dry-run
