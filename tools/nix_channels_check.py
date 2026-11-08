#!/usr/bin/env python3
"""Validate every Nix surface of this repository. Stdlib only.

Surfaces, all pinned to the same nixpkgs revision (flake.lock):
  flake   nix build .#topmatic
  compat  nix-build the root default.nix
  nur     nix-build repos/<owner>/default.nix the way NUR evaluates it

Fail-closed: any surface that fails to build or smoke-test (binary
reports a version different from Cargo.toml) aborts with exit 1.
"""
from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OWNER = "c4i0b"


def die(msg: str) -> None:
    print(f"abort: {msg}", file=sys.stderr)
    sys.exit(1)


def run(cmd: list[str], **kw) -> subprocess.CompletedProcess:
    print(f"$ {' '.join(cmd)}")
    return subprocess.run(cmd, cwd=ROOT, check=False, **kw)


def pinned_nixpkgs() -> tuple[str, str]:
    lock = json.loads((ROOT / "flake.lock").read_text())
    node = lock["nodes"]["nixpkgs"]["locked"]
    url = f"https://github.com/NixOS/nixpkgs/archive/{node['rev']}.tar.gz"
    return url, node["narHash"]


def manifest_version() -> str:
    m = re.search(r'^version = "([^"]+)"',
                  (ROOT / "Cargo.toml").read_text(), re.M)
    if not m:
        die("no version in Cargo.toml")
    return m.group(1)


def smoke(out_link: Path, want: str) -> None:
    binary = out_link / "bin" / "topmatic"
    r = run([str(binary), "--version"], capture_output=True, text=True)
    got = (r.stdout or "").strip()
    if r.returncode != 0 or got != f"topmatic {want}":
        die(f"{out_link.name}: smoke test failed ({got!r}, want "
            f"{want!r}, exit {r.returncode})")
    print(f"ok: {out_link.name} -> {got}")


def main() -> None:
    want = manifest_version()
    url, nar = pinned_nixpkgs()
    flakes = ["--extra-experimental-features", "nix-command flakes"]

    out = Path("/tmp") / "nix-check"
    out.mkdir(exist_ok=True)

    r = run(["nix", *flakes, "build", f"{ROOT}#topmatic",
             "--no-link", "--print-out-paths"], capture_output=True, text=True)
    if r.returncode != 0:
        die(f"flake build failed:\n{r.stderr}")
    smoke(Path(r.stdout.strip()), want)

    legacy_out = out / "compat"
    r = run(["nix-build", str(ROOT), "-o", str(legacy_out)],
            capture_output=True, text=True)
    if r.returncode != 0:
        die(f"default.nix build failed:\n{r.stderr}")
    smoke(legacy_out.resolve(), want)

    expr = ("let pkgs = import (fetchTarball { url = \"%s\"; sha256 = "
            "\"%s\"; }) {}; in (import %s/repos/%s/default.nix { "
            "inherit pkgs; }).topmatic" % (url, nar, ROOT, OWNER))
    nur_out = out / "nur"
    r = run(["nix-build", "-E", expr, "-o", str(nur_out)],
            capture_output=True, text=True)
    if r.returncode != 0:
        die(f"nur expression build failed:\n{r.stderr}")
    smoke(nur_out.resolve(), want)

    print(f"all nix surfaces green on topmatic {want} "
          f"(nixpkgs {url.rsplit('/', 1)[-1].removesuffix('.tar.gz')[:12]})")


if __name__ == "__main__":
    main()
