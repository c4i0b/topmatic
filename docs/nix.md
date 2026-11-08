# Nix distribution (flake) and release interplay

The repo ships `flake.nix` + `nix/package.nix`
(`rustPlatform.buildRustPackage`, version read from `Cargo.toml`), so
`nix run github:c4i0b/topmatic` works. A root `default.nix` makes the
same package reachable without flakes: `nix-build` / `nix-shell -p
(import ./. {})` on any Nix.

## Three channels, three mechanisms

| Channel | Who updates it | On release |
|---|---|---|
| AUR (`.github/workflows/aur.yml`) | our workflow, gated on `AUR_ENABLED` | automatic on `release: published` |
| nixpkgs (`topmatic` package) | r-ryantm bot via `passthru.updateScript` | automatic after the PR merges |
| flake (`nix/package.nix`) | us | CI-enforced, manual `just flake-hash` |

## The flake cargoHash rule

`cargoHash` covers the vendored crates from `Cargo.lock`:

- version-only releases (`ship prepare` bumps only the own-package
  entries): hash unchanged, nothing to do
- dependency changes (the weekly bump flow): hash must be regenerated
  with `just flake-hash` (runs the mismatch loop inside the `nixbox`
  distrobox; idempotent)

## How the release skill enforces it

CI has a `nix` job that builds the flake on every push/PR. The
`ship publish` gate (`ci_green_on`) requires every run on the release
commit to be green, so a stale `cargoHash` blocks the release
fail-closed — no release.py changes needed.

## Validation environment: throwaway Nix container

`just flake-hash` uses a disposable `nixos/nix` container (has git and
Nix preinstalled), in line with the project's ephemeral tooling model:
nothing persists, the host performs the only file edit (`sed` on
`nix/package.nix`), and the container is discarded. On abort the file
is restored from git, fail-closed.

Recurring shape for ad-hoc validation:

    podman run --rm -v "$PWD":/src:Z docker.io/nixos/nix \
      nix --extra-experimental-features "nix-command flakes" \
      build /src#topmatic --option sandbox false

Tradeoff vs a persistent box: every run recompiles from a cold store
(a few minutes); in exchange there is no state to clean up and no
divergence between machines. A persistent distrobox (`nixbox`) is a
fine personal accelerator but nothing depends on it anymore.

## Known upstream quirk

`topgrade` at nixpkgs master HEAD can be missing from
`cache.nixos.org` with a local `aws-lc-sys` build failure
(`AWS_LC_SYS_USE_SYSTEM=1` without a system AWS-LC). The flake pins
`nixos-unstable`, which has a cached `topgrade` 17.15.0 (the version
this project aligns with). Prefer validating against the pinned
revision, not master HEAD.
