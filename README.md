# topmatic

User-level scheduled updates for Linux, powered by [topgrade](https://github.com/topgrade-rs/topgrade) as the backend. A ratatui TUI (plus CLI) that programs update jobs — like daily Flatpak updates with auto-clean — without sudo and without touching your own topgrade configuration.

Works on any distro that has topgrade installed and a systemd user session.

## How it works

- **topgrade stays the backend**: topmatic composes `topgrade --config <topmatic-owned> --only <steps> --cleanup --no-ask-retry --yes`. Your `~/.config/topgrade.toml` is never read — the isolated config lives at `~/.config/topmatic/topgrade.toml`.
- **Declarative config**: `~/.config/topmatic/config.toml` is the source of truth. The TUI, `topmatic edit` (opens `$EDITOR`) and hand edits are all first-class: every TUI open or save runs a sync that converges systemd to the config and removes orphan timers.
- **systemd user timers, minimal footprint**: everything systemd *mandates* lives in `~/.config/systemd/user/` — two shared template units written once (`topmatic@.service`, `topmatic@.timer`) plus one 3-line drop-in per profile (schedule only). Everything else (steps, notify policy, cleanup, topgrade config) stays under `~/.config/topmatic/`.
- **Sane defaults**: new profiles use a **daily spread** schedule — `OnCalendar=daily` + `RandomizedDelaySec=12h` — firing once a day at a uniformly random time (no herd, no privileged hour like a fixed noon). Timers are `Persistent=true`, and the service runs at minimum priority (`Nice=19`, batch CPU, idle IO) so it never competes with your work.
- **Runner**: headless `topmatic run <profile>` takes an flock (no overlapping runs), tees output to timestamped logs under `~/.local/state/topmatic/logs/`, writes a `last-run` JSON status and notifies via `notify-send` according to the profile policy (default: only on failure).
- **No sudo, anywhere**: enabling linger is the only privileged-adjacent operation and topmatic never escalates: it calls `loginctl enable-linger` for your own user (polkit allows self-linger on standard distros) and, if that is denied, it only *suggests* the command for you to run.

## Requirements

- Linux with systemd (user session)
- [topgrade](https://github.com/topgrade-rs/topgrade) in PATH
- optional: `notify-send` for desktop notifications
- recommended: `loginctl enable-linger` so timers fire without an open session (topmatic shows the state and can enable it with `L`)

## Install

```sh
cargo install --path .
```

## Usage

Run `topmatic` for the TUI:

| Key | Action |
|-----|--------|
| `n` / `e` | new / edit profile |
| `space` | pause / resume |
| `d` | delete (warns: run history is purged too) |
| `r` | run now via systemd |
| `t` | dry-run test |
| `l` | browse run logs |
| `L` | enable lingering |
| `R` | resync units to config |
| `?` | help |

Profile editor: steps picker with incremental search (curated user-level categories first, full `topgrade --only` catalog parsed from your installed topgrade), schedule presets — hourly, every N hours, daily at fixed time, weekly, **spread** (anchor + random window, the default), custom `OnCalendar` validated with `systemd-analyze calendar` — plus cleanup toggle (default on) and notification policy.

CLI:

```sh
topmatic list                 # profiles, next run, last status
topmatic sync                 # converge systemd units to config
topmatic edit                 # $EDITOR on config, then sync
topmatic run <profile>        # headless run (what systemd units call)
topmatic run <profile> --dry-run
```

## File layout

```
~/.config/topmatic/config.toml                      # profiles (source of truth)
~/.config/topmatic/topgrade.toml                    # topmatic-owned topgrade config
~/.config/systemd/user/topmatic@.service|timer      # shared templates (written once)
~/.config/systemd/user/topmatic@<p>.timer.d/        # per-profile 3-line schedule drop-in
~/.local/state/topmatic/logs/<profile>/             # run logs
~/.local/state/topmatic/status/<profile>.json       # last-run status
```

## Known behavior

- The topgrade `flatpak` step updates both user and system installations; system-wide updates rely on polkit and may fail (or need a GUI auth agent) when unattended. Failures surface in the log, status and notification.
- `system` scope jobs (privileged updates) are modeled (`Scope::System`) but intentionally not implemented yet; the sync reports them as unsupported.

## Development

Task runner is [just](https://github.com/casey/just) (`justfile`):

```sh
just            # list recipes
just check      # full gate: fmt --check + clippy -D warnings + test
just one <name> # single test
just deploy     # release build + install to ~/.cargo/bin
just image && just container-gate   # gate inside the devcontainer image
just verify <profile>               # host-side dry-run
just fixture    # regenerate tests/fixtures/topgrade_help.txt
```

- Devcontainer included (`.devcontainer/`, Rust + beads).
- Task tracking with [beads](https://github.com/gastownhall/beads): `bd ready`, `bd prime`.
- Tests follow the pyramid: fast unit tests for domain logic (schedules, argv, catalog parsing), integration tests with stub `topgrade`/`notify-send` binaries, and a small set of headless end-to-end checks. The TUI itself is covered by a manual checklist:

  - [ ] open TUI with no config → empty dashboard, templates installed, linger hint
  - [ ] `n` → create profile with steps + schedule → timer appears in `systemctl --user list-timers`
  - [ ] `t` dry-run → status message ok; `l` shows the log
  - [ ] `space` pause → timer disabled; resume → re-enabled
  - [ ] `d` delete → timer gone, logs purged
  - [ ] edit `~/.config/topmatic/config.toml` by hand → reopen TUI → changes reconciled

## Roadmap

- v0.2: interactive PTY run inside the TUI, log retention
- v0.3: cron fallback for non-systemd distros, `topmatic doctor`
- v0.4: packaging (crates.io, AUR, release binaries), pt-BR i18n
- later: `Scope::System` jobs (system-level updates via systemd system units)

## License

MIT
