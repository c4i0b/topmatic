# Environment

The ephemeral tooling model: the host provides only `just` and `podman`.
Everything else — Rust toolchain, rustfmt, clippy, systemd-analyze — lives in
the image. If a recipe needs a tool that is not in the image, that is a
Containerfile bug, not an install-at-runtime.

- `just image` builds `topmatic/tooling:dev` (idempotent, layer-cached).
  `just shell` opens an interactive container.
- Every recipe runs via `podman compose run --rm tooling`. No state survives
  a run; the cargo cache lives in a named volume.
- `TOPMATIC_DIRECT=1` bypasses the runner so CI (or any host with its own
  toolchain) runs recipes directly. The Containerfile is the source of truth
  for the package list when someone wires CI (none exists today).

## Host-side recipes

`deploy`, `verify`, `smoke`, `fixture` and `fixture-disable` need the real
host: installed topmatic, topgrade in PATH, a real TTY for the TUI, python3.
They run directly, never through the container.

## Dependency bumps

Weekly GitHub Action (`.github/workflows/bump.yml`): runs
`tools/bump.py` (from the dependency-bump skill) with the runner's token
and merges the PR after the full gate passes IN THE SAME JOB — events
created with GITHUB_TOKEN do not trigger the ci workflow on the PR
branch, so a required branch check would stall the auto-merge forever.
Local `just bump` runs the same flow in the container but stops at
`git push`: the gh token is keyring-based and unreachable from containers
(by model design); the Action path is the automated one.


## Releases

`tools/release.py` (from the release skill) drives three stages through
`just ship`:

- `just ship plan --bump patch|minor|major|X.Y.Z` — read-only JSON state
  (versions, tag, changelog section, tree).
- `just ship prepare --bump X.Y.Z` — bumps Cargo.toml, refreshes
  Cargo.lock via `cargo update --workspace` (never regex-bumped), dates
  the `## [Unreleased]` changelog section and creates the
  `release: vX.Y.Z` commit. Write the `[Unreleased]` section first:
  release notes come from CHANGELOG.md (keep-a-changelog).
- `just ship publish --bump X.Y.Z --yes` — the irreversible stage, gated
  fail-closed: clean tree, tag absent, changelog section, HEAD is the
  release commit, and CI green on that exact SHA. Zero CI runs on the
  commit is a refusal by design, so the release commit must be pushed
  and green before publish.

crates.io publishing (`cargo publish`) is deliberately outside the
script for now.

## The live systemd bridge

The user systemd manager cannot run inside the container (needs the host
session). `compose.yml` bridges it instead: the host session bus socket is
mounted read-write at its own path, `label=disable` keeps SELinux from
blocking the socket, and recipes that need `systemctl --user` create
`/run/systemd/system` first (systemctl checks `sd_booted()` before it even
connects; missing dir reports `offline`).

Two hard-won details, do not "simplify" them away:

- Mount ONLY the `bus` socket file, not the whole runtime dir. As container
  uid 0, sd_bus prefers the manager's `systemd/private` socket, whose
  handshake fails from the container (`ENODATA`); with the private socket
  absent it falls back to the session bus, which works.
- The image still bakes real `systemd-analyze`: unit tests validate
  OnCalendar expressions through it, and silent-skip when it is missing was
  a real bug (a green suite with the validation never exercised).
