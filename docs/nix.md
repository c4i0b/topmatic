# Nix packaging (release-pinned flake)

The Nix channel serves the **latest published release**, not main.
`nix/package.nix` owns the whole pinned package: `version`,
`fetchFromGitHub` on the `v${version}` tag, the source `hash` and the
`cargoHash`. The flake and the root `default.nix` are thin entry
points that `callPackage` the same expression, so
`nix run github:c4i0b/topmatic`, `nix profile install` and plain
`nix-build` all build the identical release.

## How releases advance the channel

`release: published` triggers the `update` job of
`.github/workflows/nix.yml` (the same file guards manual expression
changes with the `validate` job on push/PR): it verifies the tagged
`Cargo.toml` matches the tag, runs `nix-update` (updates `version`,
source hash and `cargoHash`, with a verification build), and commits
the expression to main. Prereleases are excluded.
Nothing else touches the nix files: the weekly dependency bump has no
nix responsibilities (the pinned lockfile only changes at release
time), and `flake.lock` (the nixpkgs pin) is not coupled to releases.

## The three hashes

| Hash | Covers | Changes when |
|---|---|---|
| `hash` (src) | extracted release source | every release (nix-update) |
| `cargoHash` | vendored crates of the release `Cargo.lock` | release lockfile changes (nix-update) |
| `flake.lock` narHash | the nixpkgs that builds it | deliberate nixpkgs upgrades |

## Version semantics

The version pinned in `nix/package.nix` is the channel's source of
truth; `Cargo.toml` remains the project's. They synchronize at release
time (the workflow verifies them). Consequence: between releases,
main's `Cargo.toml` may be ahead (e.g. a prepared-but-unpublished
release) while the channel still serves the last release. Documented
for users in the README.

## Validation

`tools/nix_check.py` builds the flake and the `default.nix`
shim against the `flake.lock`-pinned nixpkgs and smoke-tests the
binary version against the pinned `nix/package.nix` version. CI runs
it on pushes/PRs touching the nix surfaces; locally:

    podman run --rm -v "$PWD":/src:Z --workdir /src docker.io/nixos/nix \
      sh -c 'nix --extra-experimental-features "nix-command flakes" \
      shell nixpkgs#python3 --command python3 tools/nix_check.py'

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
- The nixpkgs submission for this project is parked (closed by the
  new-package maturity policy) on fork branch `add-topmatic`; its
  expression is nearly identical to `nix/package.nix`.
