# topmatic

Your user-level tools updated automatically without sudo.

![dashboard](docs/assets/dashboard.png)

Flatpaks, cargo installs, npm globals, pipx apps. [topgrade](https://github.com/topgrade-rs/topgrade) already knows how to update all of them. topmatic turns topgrade into scheduled jobs you manage from a TUI.

## Why

- Automatic updates of **your** software without root, daemons or cron hacks
- Your own topgrade config stays untouched. topmatic always runs topgrade with an isolated config
- Missed runs catch up on the next boot (`Persistent=` timers)
- Jobs run at minimum priority (`Nice=19`, batch CPU, idle IO)
- Schedules anchor at midnight with a small random delay (≤5min) so nothing hammers mirrors at the same second
- Flaky runs self-heal. Steps retry immediately and failed runs retry behind a connectivity check
- Every start repairs drift: unit files, stray overrides, orphan timers. `topmatic doctor` diagnoses and `topmatic reset` starts clean

## Requirements

- Linux with **systemd** (user session). systemd is the only scheduler supported today
- [topgrade](https://github.com/topgrade-rs/topgrade) in `PATH` (`cargo install topgrade` or your distro package)
- Rust toolchain to install from source

## Install

```sh
cargo install --git https://github.com/c4i0b/topmatic
```

- Run `topmatic` once. It installs the user units and converges the timers
- To let timers fire while logged out enable lingering yourself: `loginctl enable-linger`. topmatic never runs anything privileged

## Use

```sh
topmatic          # TUI
topmatic list     # profiles, next run, last status
topmatic doctor   # diagnose and repair unit drift
topmatic doctor --repair  # quarantine a broken config and start fresh
topmatic sync     # converge systemd units to the config
topmatic edit     # back up the config, edit it with $EDITOR, then sync
topmatic run <profile> [--dry-run]
topmatic reset [--all]   # remove units, schedules and history. --all also archives the config
```

### Presets as overlays

- Picking a preset activates it immediately under its suggested name. No name to type, no editor to cross
- `All` runs everything installed with no step list and picks up steps topgrade gains later
- Editing a preset profile works like any other, but the config keeps only what you changed
- `extra_steps` and `excluded_steps` adjust the selection. A reverted override disappears from the file
- Custom profiles start from the picker's custom entry and select steps from the live topgrade catalog

### Keys

The footer always shows them per screen:

- Home: `↑↓ move`, `n` new, `e` edit, `d` delete, `r` run now, `l` logs, `/` filter, `?` help, `q` quit
- Editor: `↑↓←→ move`, `enter` toggle or choose, `ctrl+a` all, `ctrl+d` none, `tab` section, `ctrl+s` save from anywhere, `esc` back
- Pressing `r` opens the live log. `x` stops the run, `esc` goes back, any key keeps the view open. Logs: `enter` open, `r` refresh, `esc` back
- Mouse click and scroll work. `L` toggles the activity panel. `?` opens the full key map anywhere

The editor cycles steps, schedule and options with `Tab`. Schedules anchor at midnight: daily, every 6 or 12 hours, weekly, every 2 weeks or monthly.

![editor](docs/assets/editor.png)

Press `?` for the full key map and what lingering means:

![help](docs/assets/help.png)

Every run leaves a log you can browse with `l`:

![logs](docs/assets/logs.png)

## Config

- Profiles live in `~/.config/topmatic/config.toml`. Edit it by hand or via TUI, both are first class
- A `[defaults]` table tunes run behavior for every profile: `retries`, `retry_delay`, `give_up_after`, `network_wait`, `random_delay` (`2min`-style values)
- `config.example.toml`, regenerated next to it, documents every key

### Schedules as dotfiles

- `config.toml` is the single source of truth and safe to share between machines. Restore it on a new machine and run `topmatic` once
- Do **not** share `~/.local/state/topmatic` (run history and logs are machine local) or `~/.config/systemd/user`. Both regenerate automatically

### Sync safety

- A timer without a profile but **with** topmatic's drop-in is an orphan and gets cleaned up
- A `topmatic@*` timer **without** the drop-in belongs to something else. topmatic leaves it alone and just reports it

## Development

```sh
just check  # fmt + clippy + tests
```

`just --list` shows every recipe. Tests use stubs instead of real topgrade/systemd so they run anywhere a Rust toolchain exists. The exceptions are `tests/systemd_integration.rs` (needs a real user systemd, skips otherwise) and `just smoke` (drives the real TUI in a pty).

## License

MIT
