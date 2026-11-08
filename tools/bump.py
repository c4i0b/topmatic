#!/usr/bin/env python3
"""Stack-agnostic dependency bump.

Detects package manifests and image tags, runs each stack's native
updater, fixes non-current image tags, runs the repo's cheap checks
(just lint / just test when present) and opens a PR that auto-merges
once branch CI is green.

Stdlib only (runs on python-3.15 in wolfi-base; no pip install step).
Flow and philosophy: see SKILL.md next to this script.
"""
from __future__ import annotations

import argparse
import datetime
import re
import shutil
import subprocess
import sys
from pathlib import Path

# Current majors — keep in sync with the Freshness list in
# ephemeral-tooling model references/base-images.md (single source).
CURRENT = {
    "debian": "13",
    "alpine": "3.24",
    "node": "26",
    "python": "3.15",
    "golang": "1.27",
    "rust": "1.99",
    "ubi": "10",
}
OK_TAGS = {"latest", "latest-dev", "rawhide"}
# Debian codenames are canonical style; map to the current number,
# preserving suffixes (bookworm-slim -> 13-slim, never bare 13)
DEBIAN_CODENAMES = {"bookworm": "12", "trixie": "13", "bullseye": "11"}
# Image family by last path segment of the reference.
FAMILY_ALIASES = {"golang": "golang", "go": "golang"}

# Lockfile-driven updaters: run in the tooling container; skipped (with a
# warning) when the tool is not baked into the image.
UPDATERS = [
    ("uv.lock", ["uv", "lock", "--upgrade"]),
    ("poetry.lock", ["poetry", "update"]),
    ("package-lock.json", ["npm", "update"]),
    ("pnpm-lock.yaml", ["pnpm", "update"]),
    ("yarn.lock", ["yarn", "upgrade"]),
    ("go.mod", ["sh", "-c", "go get -u ./... && go mod tidy"]),
    ("Cargo.lock", ["cargo", "update"]),
    ("Gemfile.lock", ["bundle", "update", "--conservative"]),
]

TAG_RE = re.compile(r"(?P<ref>\S+):(?P<tag>[A-Za-z0-9._-]+)")
# FROM, compose image:, and ARG NAME=ref — ARG indirection is where the
# real tag usually lives when FROM uses a variable (observed: finance)
TAG_LINE_RE = re.compile(
    r"^\s*(FROM\s+\S+|image:\s*\S+|ARG\s+[A-Za-z_]\w*=\S+)")

# Families versioned as major.minor (python, golang, alpine, rust) vs a
# single major (node, debian, ubi): decides what "current" means.
XY_FAMILIES = {"alpine", "python", "golang", "rust"}

MAJOR_MINOR = re.compile(r"^(\d+)\.(\d+)(?P<rest>.*)$")
MAJOR_ONLY = re.compile(r"^(\d+)(?P<rest>.*)$")


MANIFESTS = {"pyproject.toml", "package.json", "go.mod", "Cargo.toml",
             "Gemfile"}
NAME_RE = re.compile(r'^[-+ ]name = "([^"]+)"')
VER_RE = re.compile(r'^([-+])version = "([^"]+)"')


def run(cmd: list[str], **kw) -> subprocess.CompletedProcess:
    print(f"  $ {' '.join(cmd)}")
    return subprocess.run(cmd, check=False, **kw)


def is_current(family: str, tag: str) -> bool:
    m = MAJOR_MINOR.match(tag)
    if m:
        if family in XY_FAMILIES:
            return f"{m.group(1)}.{m.group(2)}" == CURRENT[family]
        return m.group(1) == CURRENT[family].split(".")[0]
    m = MAJOR_ONLY.match(tag)
    if m:  # bare major floats with the latest of that line
        return m.group(1) == CURRENT[family].split(".")[0]
    return False  # codename: manual review


def current_tag(family: str, tag: str) -> str:
    """Current replacement for `tag`, preserving suffixes (e.g. -slim)."""
    m = MAJOR_MINOR.match(tag)
    if m and family in XY_FAMILIES:
        return CURRENT[family] + m.group("rest")
    m = MAJOR_ONLY.match(tag)
    if m:
        return CURRENT[family].split(".")[0] + m.group("rest")
    return CURRENT[family]


# Image family by last path segment (digit-stripped: ubi9 -> ubi; the
# version can live in the repo name, not the tag).
def family_of(ref: str) -> str | None:
    parts = ref.split("/")
    for name in (parts[-1], parts[-2] if len(parts) > 1 else ""):
        for cand in (name, re.sub(r"\d+$", "", name)):
            cand = FAMILY_ALIASES.get(cand, cand)
            if cand in CURRENT:
                return cand
    return None


def repo_name_major(ref: str) -> tuple[str, str] | None:
    """(family, major) when the version lives in the repo name (ubi9)."""
    m = re.search(r"(\d+)$", ref.split("/")[-1])
    if not m:
        return None
    fam = family_of(ref)
    return (fam, m.group(1)) if fam else None


def fix_image_tags(root: Path, dry: bool) -> tuple[list[str], list[tuple], bool]:
    """Rewrite non-current numeric tags in Containerfile/compose.yml.

    Returns (findings, rewrites, modified) where rewrites are
    (file:line, image, old_tag, new_tag)."""
    findings: list[str] = []
    rewrites: list[tuple] = []
    modified = False
    for name in ("Containerfile", "Dockerfile", "compose.yml", "compose.yaml"):
        f = root / name
        if not f.is_file():
            continue
        lines = f.read_text().splitlines(keepends=True)
        changed = False
        for i, line in enumerate(lines):
            if not TAG_LINE_RE.match(line):
                continue
            if line.lstrip().startswith("FROM"):
                bare = line.split()[1]
            elif line.lstrip().startswith("ARG"):
                bare = line.split("=", 1)[1].strip().strip("'\"")
            else:  # image: <ref>[:tag]
                bare = line.split(":", 1)[1].strip().strip("'\"")
            bare = bare.split("@", 1)[0]  # digest pin: @sha256:hex would
            #                       otherwise parse the hex as a tag
            if not bare:
                continue
            m = TAG_RE.search(bare)
            ref, tag = (m.group("ref"), m.group("tag")) if m else (bare, None)
            if tag is None or tag in OK_TAGS or tag == "dev":
                # version can live in the repo name (ubi9); report only —
                # rewriting a registry path is riskier than a tag
                rn = repo_name_major(ref)
                if rn and rn[1] != CURRENT[rn[0]].split(".")[0]:
                    cur = CURRENT[rn[0]].split(".")[0]
                    findings.append(
                        f"{name}:{i + 1}: {ref} carries major {rn[1]} "
                        f"({rn[0]} current: {cur}) - review by hand")
                continue
            codename = next((c for c in DEBIAN_CODENAMES
                             if family_of(ref) == "debian"
                             and tag.startswith(c)), None)
            orig_tag = tag
            if codename:
                tag = tag.replace(codename, DEBIAN_CODENAMES[codename], 1)
            family = family_of(ref)
            if family is None or family not in CURRENT:
                # report only image-shaped refs: explicit tag or registry
                # path — plain ARG scalars (VERSION=1.22) are not images
                if m is not None or "/" in ref:
                    findings.append(f"{name}:{i + 1}: untracked image "
                                    f"{ref}:{tag} - check currency by hand "
                                    "(base-images.md)")
                continue
            if not is_current(family, tag):
                new = current_tag(family, tag)
                findings.append(f"{name}:{i + 1}: {ref}:{orig_tag} -> :{new}")
                rewrites.append((f"{name}:{i + 1}", ref, orig_tag, new))
                if not dry:
                    lines[i] = line.replace(f"{ref}:{orig_tag}",
                                            f"{ref}:{new}")
                    changed = True
            elif tag not in (CURRENT[family], f"{CURRENT[family]}-slim",
                             CURRENT[family].split(".")[0]):
                findings.append(f"{name}:{i + 1}: {ref}:{tag} non-standard "
                                f"for {family} (current: {CURRENT[family]}) - "
                                "review by hand")
        if changed:
            (root / name).write_text("".join(lines))
            modified = True
    return findings, rewrites, modified


def run_updaters(root: Path) -> bool:
    """Run every applicable native updater; True if any file changed."""
    dirty_before = git(root, "status", "--porcelain").stdout
    for marker, cmd in UPDATERS:
        if not (root / marker).is_file():
            continue
        tool = shutil.which(cmd[0]) if cmd[0] != "sh" else "/bin/sh"
        if tool is None:
            print(f"WARNING: {marker} present but '{cmd[0]}' not in image; "
                  f"skipping (missing tool = Containerfile bug)")
            continue
        r = run(cmd, cwd=root)
        if r.returncode != 0:
            print(f"WARNING: updater for {marker} failed "
                  f"(rc={r.returncode}); continuing")
    return git(root, "status", "--porcelain").stdout != dirty_before


def git(root: Path, *args: str) -> subprocess.CompletedProcess:
    return subprocess.run(["git", "-C", str(root), *args],
                          capture_output=True, text=True, check=False)


def has_recipe(root: Path, name: str) -> bool:
    j = root / "justfile"
    if not j.is_file():
        return False
    return re.search(rf"^{name}\s*:", j.read_text(), re.M) is not None


def cheap_checks(root: Path) -> bool:
    """The repo's own cheapest gates: just lint + just test when they
    exist (they dispatch into the tooling container themselves). The
    full matrix stays on branch CI — that is what gates the merge."""
    ok = True
    for recipe in ("lint", "test"):
        if has_recipe(root, recipe):
            r = subprocess.run(["just", recipe], cwd=root, check=False)
            print(f"  $ just {recipe} (rc={r.returncode})")
            ok &= r.returncode == 0
        else:
            print(f"NOTE: no '{recipe}' recipe; skipping")
    if not (has_recipe(root, "lint") or has_recipe(root, "test")):
        print("WARNING: no cheap checks found (no justfile recipes); "
              "branch CI is the only gate")
    return ok


def bump_type(old: str, new: str) -> str:
    """major/minor/patch from the leading numeric parts (Renovate-style
    update type; majors get a highlight section in the PR body)."""
    def parts(v: str) -> list[int]:
        out = []
        for tok in re.split(r"[.\-+]", v):
            if tok.isdigit():
                out.append(int(tok))
            else:
                break
        return out
    o, n = parts(old), parts(new)
    for i, (a, b) in enumerate(zip(o, n)):
        if a != b:
            return "major" if i == 0 else "minor" if i == 1 else "patch"
    if len(n) > len(o):
        return "minor"
    return "unknown"


def summarize_toml_lock_diff(diff: str) -> list[tuple[str, str, str]]:
    """uv.lock/Cargo.lock: pair `name =` context with -/+ `version =`."""
    out, name, old_ver = [], None, None
    for line in diff.splitlines():
        m = NAME_RE.match(line)
        if m:
            name = m.group(1)
            continue
        m = VER_RE.match(line)
        if m:
            if m.group(1) == "-":
                old_ver = m.group(2)
            elif old_ver and name:
                out.append((name, old_ver, m.group(2)))
                old_ver = None
    return out


def summarize_json_lock(root: Path, base: str, path: str) -> list[tuple[str, str, str]]:
    """package-lock.json: compare resolved versions per package."""
    import json

    def versions(rev: str) -> dict:
        blob = git(root, "show", f"{rev}:{path}").stdout
        return _pkg_versions(blob)

    def versions_from_disk(root: Path, path: str) -> dict:
        try:
            return _pkg_versions((root / path).read_text())
        except OSError:
            return {}

    old, new = versions(base), versions_from_disk(root, path)
    return [(k, old[k], new[k])
            for k in old.keys() & new.keys() if old[k] != new[k]]


def _pkg_versions(blob: str) -> dict:
    import json

    try:
        return {k.rsplit("node_modules/", 1)[-1].strip("/"):
                v.get("version", "?")
                for k, v in json.loads(blob).get("packages", {}).items()
                if k}
    except (json.JSONDecodeError, AttributeError):
        return {}


def collect_deps(root: Path, base: str) -> list[tuple[str, str, str]]:
    """Version-level changes across all lockfiles in the diff — used both
    to gate (files changed is NOT a bump; uv can rewrite a lockfile's
    content-hash with zero upgrades, observed in finance) and for the PR
    body."""
    deps: list[tuple[str, str, str]] = []
    for f in git(root, "diff", "--name-only", base).stdout.split():
        if f == "package-lock.json":
            deps += summarize_json_lock(root, base, f)
        elif f.endswith(".lock"):
            deps += summarize_toml_lock_diff(
                git(root, "diff", base, "--", f).stdout)
    return deps


def build_pr_body(root: Path, base: str, rewrites: list[tuple],
                  findings: list[str]) -> str:
    """Lean PR body: one-line summary, single from/to table (majors
    first with ⚠️/bold), review findings, manifest diffs, footer."""
    deps: list[tuple[str, str, str, str]] = []  # name, old, new, type
    sections: list[str] = []
    files = git(root, "diff", "--name-only", base).stdout.split()
    for f in files:
        if f in MANIFESTS:
            d = git(root, "diff", base, "--", f).stdout
            sections += [f"### Manifest changes — `{f}`",
                         "```diff", *d.splitlines()[:60], "```", ""]
        elif f == "package-lock.json":
            deps += [(n, o, n2, bump_type(o, n2)) for n, o, n2 in
                     summarize_json_lock(root, base, f)]
        elif f.endswith(".lock"):
            s = summarize_toml_lock_diff(
                git(root, "diff", base, "--", f).stdout)
            if s:
                deps += [(n, o, n2, bump_type(o, n2)) for n, o, n2 in s]
            else:
                sections += [f"### {f}", "- no version-level summary; "
                             "see diff", ""]

    n_major = sum(1 for d in deps if d[3] == "major")
    summary = []
    if deps:
        summary.append(f"{len(deps)} dependenc"
                       f"{'ies' if len(deps) > 1 else 'y'}"
                       + (f", {n_major} major" if n_major else ""))
    if rewrites:
        summary.append(f"{len(rewrites)} image tag"
                       f"{'s' if len(rewrites) > 1 else ''}")

    lines: list[str] = []
    if summary:
        lines += ["Updates " + " and ".join(summary) + ".", ""]
    rows = [(f"⚠️ {n}", o, f"**{nv}**") for n, o, nv, t in deps
            if t == "major"]
    rows += [(n, o, nv) for n, o, nv, t in deps if t != "major"]
    rows += [(f"{ref} ({loc})", old, new) for loc, ref, old, new in rewrites]
    if rows:
        lines += ["| What | From | To |", "|---|---|---|",
                  *(f"| {w} | {o} | {t} |" for w, o, t in rows), ""]
    review = [x for x in findings if " -> " not in x]
    if review:
        lines += ["### Not bumped (manual decision needed)",
                  *(f"- {x}" for x in review), ""]
    if not rows:
        lines.append(git(root, "diff", "--stat", base).stdout + "")
    lines += sections
    lines += ["---", "🤖 local checks: lint/test"]
    return "\n".join(lines)


def action_required(what: str, bullets: list[str], details: str) -> str:
    """Scannable warning: heading, bold WHAT, 1-3 bullets, details
    pointer. The reader must grasp the situation without parsing a
    run-on sentence (first third of the body)."""
    return "\n".join([
        "## ⚠️ Not merged automatically — action required", "",
        f"**{what}**", "",
        *(f"- {b}" for b in bullets),
        "",
        f"Details: {details}",
    ])


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--dry-run", action="store_true",
                    help="report only; no updaters, no commit, no PR")
    ap.add_argument("--no-merge", action="store_true",
                    help="open the PR but do not request auto-merge")
    ap.add_argument("--no-pr", action="store_true",
                    help="leave the branch pushed, no PR (for manual work)")
    args = ap.parse_args()

    root = Path.cwd()
    while not (root / ".git").exists():
        if root.parent == root:
            sys.exit("FAIL: not inside a git repository")
        root = root.parent

    print("== image tags ==")
    base = git(root, "rev-parse", "HEAD").stdout.strip()
    findings, rewrites, tags_modified = fix_image_tags(root, args.dry_run)
    for f in findings:
        print(f"  {f}")

    if args.dry_run:
        for marker, _ in UPDATERS:
            if (root / marker).is_file():
                print(f"  would run updater for {marker}")
        print("dry run: nothing modified")
        return 0

    print("== dependency updaters ==")
    changed = run_updaters(root)
    if not (changed or tags_modified):
        print("nothing to bump; done")
        return 0
    if changed and not tags_modified and not collect_deps(root, base):
        # a rewritten lockfile with zero version moves is churn, not a
        # bump (observed: uv lock --upgrade refreshing content-hash) —
        # a PR for it would be weekly noise; revert instead. Manifest
        # edits (go.mod ranges) still count: their diff is the summary.
        touched = git(root, "diff", "--name-only", base).stdout.split()
        if not any(f in MANIFESTS for f in touched):
            git(root, "restore", ".")
            print("lockfile churn only (no version changes); reverted, "
                  "no PR")
            return 0

    print("== cheap checks (cheapest-first) ==")
    if not cheap_checks(root):
        print(action_required(
            "Cheap checks failed — this bump did NOT land.",
            ["the worktree is left dirty for inspection",
             "nothing was committed or pushed"],
            "the failing `just lint`/`just test` output above"))
        sys.exit(1)

    branch = f"bump/{datetime.date.today():%Y%m%d}"
    git(root, "checkout", "-B", branch)
    git(root, "add", "-A")
    if git(root, "diff", "--cached", "--quiet").returncode == 0:
        sys.exit("FAIL: nothing staged to commit (unexpected)")
    body = build_pr_body(root, base, rewrites, findings)
    title = "chore: bump dependencies and image tags"
    if git(root, "commit", "-m", title, "-m", body).returncode != 0:
        sys.exit("FAIL: git commit failed")
    # prune first: a leftover remote-tracking ref makes force-with-lease
    # fail with "stale info" after the branch was deleted post-merge
    git(root, "fetch", "--prune", "origin")
    if git(root, "push", "--force-with-lease", "-u", "origin",
           branch).returncode != 0:
        sys.exit("FAIL: git push failed (credentials live host-side in "
                 "the model — see SKILL.md; branch and commit are kept)")

    if args.no_pr:
        print(f"branch {branch} pushed; PR left to you")
        return 0

    pr = subprocess.run(["gh", "pr", "create", "--title", title,
                         "--body", body],
                        cwd=root, text=True)
    if pr.returncode != 0:
        sys.exit("FAIL: gh pr create failed (is gh authenticated where "
                 "credentials live? see SKILL.md)")

    if args.no_merge:
        print("PR open; auto-merge skipped")
        return 0
    m = subprocess.run(["gh", "pr", "merge", branch,
                        "--auto", "--squash", "--delete-branch"],
                       cwd=root, text=True)
    if m.returncode != 0:
        warning = action_required(
            "Auto-merge could not be armed for this bump.",
            ["branch protection may miss the required CI check, or "
             "require reviews — the author cannot self-approve",
             "the PR stays open; merge manually once CI is green"],
            "the PR checks tab and this run's output")
        body = subprocess.run(
            ["gh", "pr", "view", branch, "--json", "body", "--jq", ".body"],
            capture_output=True, text=True, cwd=root).stdout.strip()
        subprocess.run(["gh", "pr", "edit", branch, "--body",
                        warning + ("\n\n" + body if body else "")],
                       cwd=root)
        print(warning)
    return 0


if __name__ == "__main__":
    sys.exit(main())
