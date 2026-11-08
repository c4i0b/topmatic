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
`tools/bump.py` (from the dependency-bump skill) with the runner's token,
opens a PR and auto-merges once CI (`ci.yml`) is green — CI must stay a
required check with no required reviews, or the solo auto-merge stalls.
Local `just bump` runs the same flow in the container but stops at
`git push`: the gh token is keyring-based and unreachable from containers
(by model design); the Action path is the automated one.


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
