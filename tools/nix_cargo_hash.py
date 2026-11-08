#!/usr/bin/env python3
"""Refresh nix/package.nix cargoHash from the current Cargo.lock. Stdlib only.

Runs the mismatch loop against the flake: sets cargoHash to "", builds
(capture the reported `got:` hash), writes it back and verifies with a
second build. Fail-closed: restores the original hash on any failure.

Requires nix with flakes in PATH. Used by the bump flow whenever
Cargo.lock changes; also the manual fix path (docs/nix.md).
"""
from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PKG = ROOT / "nix" / "package.nix"
HASH_RE = re.compile(r'cargoHash = "([^"]*)";')
GOT_RE = re.compile(r"got:\s+(sha256-[A-Za-z0-9+/=]+)")


def nix_build() -> subprocess.CompletedProcess:
    return subprocess.run(
        ["nix", "--extra-experimental-features", "nix-command flakes",
         "build", f"{ROOT}#topmatic", "--no-link"],
        cwd=ROOT, capture_output=True, text=True, check=False)


def main() -> None:
    text = PKG.read_text()
    m = HASH_RE.search(text)
    if not m:
        sys.exit("abort: no cargoHash in nix/package.nix")
    original = m.group(1)

    PKG.write_text(HASH_RE.sub('cargoHash = "";', text, count=1))
    r = nix_build()
    if r.returncode == 0:
        PKG.write_text(HASH_RE.sub(f'cargoHash = "{original}";',
                                   PKG.read_text(), count=1))
        print(f"cargoHash already correct ({original[:19]}...); nothing to do")
        return
    got = GOT_RE.search(r.stderr or "")
    if not got:
        PKG.write_text(HASH_RE.sub(f'cargoHash = "{original}";',
                                   PKG.read_text(), count=1))
        sys.exit("abort: no cargoHash mismatch in the build output "
                 f"(the build failed for another reason):\n{r.stderr[-2000:]}")

    PKG.write_text(HASH_RE.sub(f'cargoHash = "{got.group(1)}";',
                               PKG.read_text(), count=1))
    v = nix_build()
    if v.returncode != 0:
        PKG.write_text(HASH_RE.sub(f'cargoHash = "{original}";',
                                   PKG.read_text(), count=1))
        sys.exit(f"abort: verification build failed:\n{v.stderr[-2000:]}")
    print(f"cargoHash = {got.group(1)} (verified)")


if __name__ == "__main__":
    main()
