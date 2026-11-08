#!/usr/bin/env python3
"""Release planner, preparer and gated publisher. Stdlib only.

Three stages, one publishing boundary (see SKILL.md next to this file):
  plan     read-only: current state + proposed release as JSON
  prepare  reversible: version files + changelog section + release commit
  publish  irreversible: gated on clean tree, exact-SHA CI and --yes

Fail-closed: absent evidence (no CI runs, missing gh, missing changelog
section) aborts, never passes.
"""
from __future__ import annotations

import argparse
import datetime
import json
import re
import subprocess
import sys
from pathlib import Path

SEMVER = re.compile(r"^(\d+)\.(\d+)\.(\d+)(?:-[0-9A-Za-z.-]+)?$")
TAG = re.compile(r"^v(\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?)$")

# Version file -> (reader, writer). First match per repo wins as primary.
VERSION_FILES = [
    ("Cargo.toml", re.compile(r'^(version = ")[^"]+(")', re.M)),
    ("pyproject.toml", re.compile(r'^(version = ")[^"]+(")', re.M)),
]


def read_version(root: Path) -> tuple[str, Path] | None:
    for name, rx in VERSION_FILES:
        f = root / name
        if f.is_file():
            m = rx.search(f.read_text())
            if m:
                return m.group(0).split('"')[1], f
    return None


def package_json_version(root: Path) -> tuple[str, Path] | None:
    f = root / "package.json"
    if f.is_file():
        data = json.loads(f.read_text())
        if "version" in data:
            return data["version"], f
    return None


def detect_version(root: Path) -> tuple[str, list[Path]] | None:
    hits = []
    primary = read_version(root) or package_json_version(root)
    if not primary:
        return None
    hits.append(primary[1])
    for name, rx in VERSION_FILES[1:]:
        f = root / name
        if f.is_file() and f != primary[1] and rx.search(f.read_text()):
            hits.append(f)
    pj = root / "package.json"
    if pj.is_file() and pj not in hits:
        data = json.loads(pj.read_text())
        if "version" in data:
            hits.append(pj)
    return primary[0], hits


def bump(version: str, part: str) -> str:
    m = SEMVER.match(version)
    if not m:
        sys.exit(f"error: current version {version!r} is not semver")
    maj, mi, pa = (int(m.group(i)) for i in (1, 2, 3))
    if part == "major":
        return f"{maj + 1}.0.0"
    if part == "minor":
        return f"{maj}.{mi + 1}.0"
    if part == "patch":
        return f"{maj}.{mi}.{pa + 1}"
    if SEMVER.match(part):
        return part
    sys.exit(f"error: invalid bump {part!r} (major/minor/patch/X.Y.Z)")


def git(root: Path, *args: str) -> subprocess.CompletedProcess:
    return subprocess.run(["git", "-C", str(root), *args], check=False,
                          capture_output=True, text=True)


def tags(root: Path) -> list[str]:
    r = git(root, "tag", "--list")
    return r.stdout.split() if r.returncode == 0 else []


def tree_clean(root: Path) -> bool:
    return git(root, "status", "--porcelain").stdout.strip() == ""


def last_tag(root: Path, version: str | None = None) -> str | None:
    rel = git(root, "tag", "--list", "v*", "--sort=-v:refname")
    for t in rel.stdout.splitlines():
        if TAG.match(t) and (version is None or t == f"v{version}"):
            return t
    return None


def changelog_section(root: Path, version: str) -> str | None:
    f = root / "CHANGELOG.md"
    if not f.is_file():
        return None
    lines = f.read_text().splitlines()
    out, capture = [], False
    head = f"## [{version}]"
    for line in lines:
        if line.startswith("## "):
            if capture:
                break
            capture = line.startswith(head)
            continue
        if capture:
            out.append(line)
    text = "\n".join(out).strip()
    return text or None


def refresh_lockfile(root: Path, files: list[Path], proposed: str) -> None:
    """Cargo.lock carries the own-package version: keep it in sync via the
    native updater instead of regex-bumping the lockfile."""
    lock = root / "Cargo.lock"
    if not (lock.is_file() and any(f.name == "Cargo.toml" for f in files)):
        return
    r = subprocess.run(["cargo", "update", "--workspace"],
                       cwd=root, capture_output=True, text=True, check=False)
    if (r.returncode != 0
            or f'version = "{proposed}"' not in lock.read_text()):
        sys.exit(f"abort: Cargo.lock not refreshed to {proposed} "
                 f"(cargo update --workspace): {r.stderr.strip()}")


def cmd_plan(root: Path, part: str) -> None:
    ver = detect_version(root)
    if not ver:
        sys.exit("error: no version file (Cargo.toml, pyproject.toml, "
                 "package.json)")
    current, files = ver
    proposed = bump(current, part)
    state = {
        "status": "ready",
        "current_version": current,
        "proposed_version": proposed,
        "target_tag": f"v{proposed}",
        "previous_tag": last_tag(root),
        "version_files": [f.name for f in files],
        "tag_exists": f"v{proposed}" in tags(root),
        "changelog_section": bool(changelog_section(root, proposed)
                                  or changelog_section(root, "Unreleased")),
        "tree_clean": tree_clean(root),
    }
    print(json.dumps(state, indent=2))


def cmd_prepare(root: Path, part: str) -> None:
    ver = detect_version(root)
    if not ver:
        sys.exit("error: no version file")
    current, files = ver
    proposed = bump(current, part)
    if f"v{proposed}" in tags(root):
        sys.exit(f"abort: tag v{proposed} already exists")
    for f in files:
        text = f.read_text()
        if f.name == "package.json":
            data = json.loads(text)
            data["version"] = proposed
            f.write_text(json.dumps(data, indent=2) + "\n")
        else:
            rx = next(rx for n, rx in VERSION_FILES if n == f.name)
            f.write_text(rx.sub(f'\\g<1>{proposed}\\g<2>', text, count=1))
    refresh_lockfile(root, files, proposed)
    ch = root / "CHANGELOG.md"
    dated = ""
    if not ch.is_file():
        sys.exit("abort: no CHANGELOG.md. Release notes come from its "
                 "sections (keep-a-changelog)")
    text = ch.read_text()
    if f"## [{proposed}]" in text:
        dated = f"section ## [{proposed}] already present"
    elif "## [Unreleased]" in text:
        today = datetime.date.today().isoformat()
        ch.write_text(text.replace(
            "## [Unreleased]", f"## [{proposed}] - {today}", 1))
        dated = f"changelog section dated {today}"
    else:
        sys.exit(f"abort: CHANGELOG.md has neither ## [Unreleased] "
                 f"nor ## [{proposed}]. Write the section first "
                 f"(keep-a-changelog, user-visible wording)")
    git(root, "add", "-A")
    r = git(root, "commit", "-m", f"release: v{proposed}")
    if r.returncode != 0:
        sys.exit(f"abort: release commit failed: {r.stderr.strip()}")
    print(f"prepared v{proposed} ({len(files)} version files, {dated})")


def ci_green_on(root: Path, sha: str) -> tuple[bool, str]:
    """Fail-closed CI evidence for one commit (gh)."""
    if subprocess.run(["sh", "-c", "command -v gh >/dev/null"],
                      capture_output=True).returncode != 0:
        return False, "gh not available"
    r = subprocess.run(
        ["gh", "run", "list", "--commit", sha, "--limit", "20",
         "--json", "status,conclusion,name"],
        capture_output=True, text=True, check=False)
    if r.returncode != 0:
        return False, f"gh failed: {r.stderr.strip()}"
    runs = json.loads(r.stdout or "[]")
    if not runs:
        return False, ("no CI runs on this commit. Events created with "
                       "GITHUB_TOKEN do not trigger workflows: run outside "
                       "Actions or with a workflow-triggering credential")
    for run in runs:
        if run["status"] != "completed":
            return False, f"run {run['name']} still {run['status']}"
    if any(run["conclusion"] != "success" for run in runs):
        return False, "a run on this commit did not succeed"
    return True, f"{len(runs)} runs green on {sha[:8]}"


def cmd_publish(root: Path, part: str, yes: bool) -> None:
    ver = detect_version(root)
    if not ver:
        sys.exit("error: no version file")
    current, _ = ver
    tag = f"v{current}"
    if not yes:
        sys.exit("abort: publish is irreversible, pass --yes")
    if not tree_clean(root):
        sys.exit("abort: working tree not clean")
    if tag in tags(root):
        sys.exit(f"abort: tag {tag} already exists (published history is "
                 "immutable: fix forward with a new version)")
    if not changelog_section(root, current):
        sys.exit(f"abort: CHANGELOG.md has no ## [{current}] section")
    sha = git(root, "rev-parse", "HEAD").stdout.strip()
    subject = git(root, "log", "-1", "--format=%s").stdout.strip()
    if subject != f"release: {tag}":
        sys.exit(f"abort: HEAD is not the release commit ({subject!r})")
    ok, why = ci_green_on(root, sha)
    if not ok:
        sys.exit(f"abort: CI gate: {why}")
    r = subprocess.run(["git", "-C", str(root), "tag", "-a", tag,
                        "-m", tag], check=False)
    if r.returncode != 0:
        sys.exit("abort: tag creation failed")
    print(f"tagged {tag} on {sha[:8]} ({why})")
    notes = changelog_section(root, current) or ""
    r = subprocess.run(
        ["gh", "release", "create", tag, "--verify-tag", "--notes", notes],
        capture_output=True, text=True, check=False)
    if r.returncode != 0:
        sys.exit(f"published tag {tag} but GitHub Release failed: "
                 f"{r.stderr.strip()}. Fix forward: create the release "
                 f"manually for the existing tag")
    print(f"released {tag}")
    subprocess.run(["gh", "release", "view", tag], check=False)


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("stage", choices=["plan", "prepare", "publish"])
    ap.add_argument("--bump", required=True,
                    help="major | minor | patch | explicit X.Y.Z")
    ap.add_argument("--yes", action="store_true",
                    help="authorize the irreversible publish stage")
    args = ap.parse_args()
    root = Path.cwd()
    if args.stage == "publish":
        cmd_publish(root, args.bump, args.yes)
    else:
        {"plan": cmd_plan, "prepare": cmd_prepare}[args.stage](root, args.bump)


if __name__ == "__main__":
    main()
