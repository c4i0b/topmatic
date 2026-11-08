# Nix packaging (flake + default.nix)

The repo ships `flake.nix` + `nix/package.nix`
(`rustPlatform.buildRustPackage`, version read from `Cargo.toml`), so
`nix run github:c4i0b/topmatic` works. The root `default.nix` exposes
the same package without flakes (`nix-build`,
`nix-shell -p (import ./.)`).

## Channel ladder

flake (canonical, pin per user) → nixpkgs (when adoption justifies
re-submission; the green expression is parked on fork branch
`add-topmatic`). Other community channels (NUR) can be added later
from a dedicated repository; nothing in this repo depends on them.

## Validating all surfaces

`tools/nix_channels_check.py` builds the flake and the `default.nix`
shim against the nixpkgs revision pinned in `flake.lock`,
smoke-testing each output against the manifest version. CI runs it on
every push/PR touching nix surfaces; locally:

    podman run --rm -v "$PWD":/src:Z --workdir /src docker.io/nixos/nix \
      sh -c 'nix --extra-experimental-features "nix-command flakes" \
      shell nixpkgs#python3 --command python3 tools/nix_channels_check.py'

## cargoHash rules

`cargoHash` in `nix/package.nix` covers the vendored crates from
`Cargo.lock`:

- version-only releases: hash unchanged, nothing to do
- dependency changes (the weekly bump flow): hash must be
  regenerated. The bump workflow does this automatically on the bump
  branch (`tools/nix_cargo_hash.py`); manually, run the same script
  with nix available (copy to container fs first: the script writes
  between builds and bind mounts trip nix's source re-hash check):

```
podman run --rm -v "$PWD":/src:Z docker.io/nixos/nix sh -c '
  cp -a /src /work && cd /work &&
  nix --extra-experimental-features "nix-command flakes" \
  shell nixpkgs#python3 --command python3 tools/nix_cargo_hash.py'
```

The script is fail-closed: it restores the original hash if the
verification build fails or no mismatch is reported.

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
