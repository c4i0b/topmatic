# topmatic

Your user-level tools, updated automatically — without sudo.

![dashboard](docs/assets/dashboard.png)

Flatpaks, cargo installs, npm globals, pipx apps… [topgrade](https://github.com/topgrade-rs/topgrade) already knows how to update all of them. topmatic turns topgrade into scheduled jobs you manage from a TUI: pick what to update, when, and forget it.

## Why

- You want automatic updates of **your** software, without root, daemons or cron hacks
- Your own topgrade config stays untouched — topmatic always runs topgrade with an isolated config
- Machines are off sometimes: timers are persistent and catch up missed runs
- Jobs run at minimum priority (`Nice=19`, batch CPU, idle IO) — never in your way
- Default schedule follows the common Linux convention: daily anchor with a small random jitter (≤30min), so nothing hammers mirrors at the same second
- Self-healing: every start rewrites drifted unit files, prunes stray schedule overrides and removes orphan timers of profiles that no longer exist; `topmatic doctor` diagnoses and `topmatic reset` starts clean
- Owns only what it proves: a timer is only ever touched if it carries topmatic's schedule drop-in (`topmatic@<profile>.timer.d/10-schedule.conf`), so timers created by other tools — even ones named `topmatic@*` — are left alone with a note instead of being deleted

## Requirements

- Linux with systemd (user session)
- [topgrade](https://github.com/topgrade-rs/topgrade) in `PATH` (`cargo install topgrade`, or your distro package manager)
- Rust toolchain (to install from source)

## Install

```sh
cargo install --git https://github.com/c4i0b/topmatic
```

Then run `topmatic` once: it installs the systemd user units and offers to enable lingering so timers fire even when you are logged out (never uses sudo).

## Use

```sh
topmatic          # TUI
topmatic list     # profiles, next run, last status
topmatic doctor   # diagnose + auto-repair unit drift
topmatic sync     # converge systemd units to the config
topmatic edit     # edit the config with $EDITOR, then sync
topmatic run <profile> [--dry-run]
topmatic reset [--all]   # remove units/schedules/history; --all also archives the config
```

In the TUI: `n` new (from a preset), `e` edit, `d` delete, `r` run now, `t` dry-run test, `l` logs, `s` resync now, `g` toggle lingering, `/` filter, mouse click/scroll, `?` help, `q` quit (lower or upper case; while you are typing in a filter, `q` is just a letter). The editor has three sections (steps, schedule, options) toggled with `Tab`; `Enter` names the profile and saves it. The right pane shows live details for the selected profile: schedule, countdown to next fire, timer state and the tail of the last run log.

New profiles start from a preset — everything user-level, dev tools or Flatpak — with steps pre-selected, a suggested name and the default schedule; tweak anything before saving. Presets are computed live from your installed topgrade, so they always match its step list.

![editor](docs/assets/editor.png)

Profiles live in `~/.config/topmatic/config.toml` — edit it by hand or via TUI, both are first-class; topmatic reconciles systemd to match it on every start.

### Schedules as dotfiles

`~/.config/topmatic/config.toml` is the single source of truth and safe to share between machines (e.g. into your dotfiles repo). On a new machine, just restore that file and run `topmatic` once: it installs the unit templates and re-creates every timer with the schedule from the config. Do **not** share `~/.local/state/topmatic` (run history and logs are machine-local) or `~/.config/systemd/user` — those units regenerate automatically, and stale symlinks are repaired by the sync.

Two safety guarantees around the sync:

- A timer without a matching profile but **with** topmatic's schedule drop-in is an orphan of a removed profile and gets cleaned up.
- A `topmatic@*` timer **without** topmatic's drop-in was created by something else — topmatic leaves it alone and just reports it, so other tools can coexist without their jobs being deleted.

## Development

```sh
just check          # fmt + clippy + tests
just container-gate # same gate inside the devcontainer image
```

Tests run anywhere (stubs instead of real topgrade/systemd). Task runner is [just](https://github.com/casey/just); `just --list` shows everything, including `just screenshots` to regenerate the README images from `docs/assets/*.tape` with [vhs](https://github.com/charmbracelet/vhs).

## License

MIT
