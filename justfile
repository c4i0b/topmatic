runner := `[ "${TOPMATIC_DIRECT:-}" = "1" ] || printf 'podman compose run --rm tooling'`

default:
    @just --list

# Build the tooling image (idempotent, layer-cached)
image:
    podman compose build

# Interactive shell in the tooling container
shell:
    podman compose run --rm tooling bash

# Full quality gate: fmt check + clippy + tests
check: fmt-check clippy test

# Cheap lint gate for the bump flow (alias)
lint: fmt-check clippy

# Bump dependencies and image tags; opens a PR that auto-merges on green CI
bump:
    {{runner}} python3 tools/bump.py

# Release flow (release skill): ship plan --bump patch | ship prepare --bump X.Y.Z | ship publish --bump X.Y.Z --yes
ship *args:
    {{runner}} python3 tools/release.py {{args}}

# Regenerate the flake cargoHash (nix/package.nix) in a throwaway Nix container; idempotent
flake-hash:
    #!/usr/bin/env bash
    set -euo pipefail
    root="{{invocation_dir()}}"
    sed -i 's|cargoHash = "[^"]*";|cargoHash = "";|' "$root/nix/package.nix"
    nix_run() {
        podman run --rm -v "$root":/src:Z docker.io/nixos/nix \
            nix --extra-experimental-features "nix-command flakes" \
            build /src#topmatic --option sandbox false "$@"
    }
    got=$(nix_run 2>&1 \
        | grep -oE "got: +sha256-[A-Za-z0-9+/=]+" | head -1 | awk "{print \$2}" || true)
    if [ -z "$got" ]; then
        git -C "$root" restore -- nix/package.nix
        echo "abort: no cargoHash mismatch reported (build failed for another reason; nix/package.nix restored)" >&2
        exit 1
    fi
    sed -i "s|cargoHash = \"\";|cargoHash = \"$got\";|" "$root/nix/package.nix"
    nix_run --no-link
    echo "cargoHash = $got (verified)"

# Run the offline test suite (container or direct via TOPMATIC_DIRECT=1)
test:
    {{runner}} cargo test

# Run a single test by name
one name:
    {{runner}} cargo test {{name}}

# Run the live systemd integration test (container bridged to the host user systemd session)
test-live:
    {{runner}} sh -c 'mkdir -p /run/systemd/system 2>/dev/null; exec cargo test --test systemd_integration -- --ignored'

# Check formatting
fmt-check:
    {{runner}} cargo fmt --check

# Fix formatting
fmt:
    {{runner}} cargo fmt

# Lint with warnings as errors
clippy:
    {{runner}} cargo clippy --all-targets -- -D warnings

# Debug build
build:
    {{runner}} cargo build

# Release build (container) and install where the systemd unit ExecStart points
deploy:
    {{runner}} cargo build --release
    mkdir -p ~/.cargo/bin
    install -m 755 target/release/topmatic ~/.cargo/bin/topmatic

# Regenerate the topgrade steps fixture from the installed topgrade (host-side)
fixture:
    topgrade --help > tests/fixtures/topgrade_help.txt

# Regenerate the accepted disable-names fixture from the real deserialize error
# (run `fixture` and `fixture-disable` with the SAME topgrade version; host-side)
fixture-disable:
    printf 'disable = ["__probe_unknown__"]\n' > /tmp/topmatic_probe.toml
    topgrade --config /tmp/topmatic_probe.toml --dry-run 2>/dev/null | head -5 > tests/fixtures/topgrade_disable_error.txt

# Host-side dry-run verification of a profile
verify profile:
    ~/.cargo/bin/topmatic run {{profile}} --dry-run

# Host smoke: drive the real TUI in a pty (see scripts/pty_drive.py)
smoke:
    python3 scripts/pty_drive.py
