# Nix packaging (flake + default.nix)

The repo ships `flake.nix` + `nix/package.nix`
(`rustPlatform.buildRustPackage`, version read from `Cargo.toml`), so
`nix run github:c4i0b/topmatic` works. The root `default.nix` exposes
the same package without flakes: `nix-build` or
`nix-shell -p (import ./.)` on any Nix installation.

## cargoHash rules

`cargoHash` in `nix/package.nix` covers the vendored crates from
`Cargo.lock`:

- version-only releases: hash unchanged, nothing to do
- dependency changes: hash must be regenerated — set it to `""`,
  build once, copy the `got:` sha256 from the mismatch error, build
  again (see the validation one-liner below)

## Validation: throwaway Nix container

    podman run --rm -v "$PWD":/src:Z docker.io/nixos/nix \
      nix --extra-experimental-features "nix-command flakes" \
      build /src#topmatic --option sandbox false

The stock `nixos/nix` image ships git (flake tree detection) and Nix;
nothing persists and the host stays clean. For `default.nix`, plain
`nix-build /src --option sandbox false` in the same container.

## Gotchas learned the hard way

- Validate against a pinned channel, not nixpkgs master HEAD:
  dependencies there can be missing from cache.nixos.org with broken
  local builds (topgrade's `aws-lc-sys` did exactly that).
- Tests must PATH-resolve any binary they exec
  (`Command::new("true")`, never `/bin/true`): `/bin/*` does not
  exist in the Nix build sandbox or in minimal Nix containers.
- Tests that exec host tools need those tools mapped into the check
  environment (`nativeCheckInputs = [ systemd ]` for
  `systemd-analyze`); `sandbox = false` locally masks this class of
  failure, a sandboxed build surfaces it.
