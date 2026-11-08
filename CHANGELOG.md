# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- Dependency refresh across the board (clap, serde_json, thiserror, libc and
  transitive crates) with no API or behavior changes.

### Internal

- Continuous integration on GitHub Actions, plus a weekly automated
  dependency-bump workflow.

## [1.2.0] - 2026-11-07

### Added

- `topmatic steps`: shows what topgrade can update at user level on your
  machine, with per-profile coverage, straight from the installed topgrade.

### Changed

- Aligned with topgrade 17.15: the built-in step catalog fallback now covers
  the full catalog, including the new user-level steps (`antigravity_ide`,
  `github_cli_skills`, `lucid_link`, `pre_commit_gc`, `tailscale`, `zed`).

### Internal

- Development moved to an ephemeral tooling container: the host needs only
  `just` and `podman`.
- The default test suite is now offline and hermetic; the live systemd layer
  is a separate opt-in (`just test-live`). Silent skips of missing tools are
  gone — a green suite always means the checks ran.

## [1.1.0] - 2026-09-13

### Changed

- Aligned with topgrade 17.11. Step names that topgrade spells differently on
  the wire are now mapped, so an exclusion such as `am` can no longer
  accidentally void the generated config.
- Containers, distrobox and toolbx are reclassified as user-level steps:
  they are selectable in profiles and run in everything mode.
- `topmatic doctor` recommends topgrade 17.11 and no longer prints the
  binary name twice.

## [1.0.3] - 2026-09-07

### Added

- topmatic now refuses to run as root. Scheduled user updates never need
  sudo, and running as root would write root-owned files into user config.

## [1.0.2] - 2026-09-07

### Changed

- Copy pass on the TUI legend and preset descriptions.

## [1.0.1] - 2026-09-07

### Changed

- Documentation and log output polish.

## [1.0.0] - 2026-09-07

### Added

- First stable release. topmatic schedules user-level updates through
  topgrade without sudo: a TUI to pick what topgrade can update on your
  machine into profiles, per-profile systemd user timers, and the `run`,
  `sync`, `list`, `doctor`, `reset` and `edit` commands. topmatic owns its
  topgrade config; your own `topgrade.toml` is never read.

[Unreleased]: https://github.com/c4i0b/topmatic/compare/v1.2.0...HEAD
[1.2.0]: https://github.com/c4i0b/topmatic/compare/v1.1.0...v1.2.0
[1.1.0]: https://github.com/c4i0b/topmatic/compare/v1.0.3...v1.1.0
[1.0.3]: https://github.com/c4i0b/topmatic/compare/v1.0.2...v1.0.3
[1.0.2]: https://github.com/c4i0b/topmatic/compare/v1.0.1...v1.0.2
[1.0.1]: https://github.com/c4i0b/topmatic/compare/v1.0.0...v1.0.1
[1.0.0]: https://github.com/c4i0b/topmatic/releases/tag/v1.0.0
