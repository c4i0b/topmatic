# topmatic

Your user-level tools updated automatically without sudo.

![dashboard](docs/assets/dashboard.png)

Flatpaks, cargo installs, npm globals, pipx apps. [topgrade](https://github.com/topgrade-rs/topgrade) already knows how to update all of them. topmatic turns topgrade into scheduled jobs you manage from a TUI. Pick what to update and when, then forget it.

## Why

- Automatic updates of **your** software without root, daemons or cron hacks
- Your own topgrade config stays untouched. topmatic always runs topgrade with an isolated config
- Machines are off sometimes: timers are `Persistent=` and catch up missed runs
- Jobs run at minimum priority (`Nice=19`, batch CPU, idle IO) so they never get in your way
- Schedules are frequencies anchored at midnight with a small random delay (≤5min) so nothing hammers mirrors at the same second
- Flaky runs self-heal. Steps retry immediately, failed runs retry behind a connectivity check, and you only hear about a failure that survived all of it
- Every start repairs drift: unit files are rewritten, stray overrides pruned, orphan timers removed. `topmatic doctor` diagnoses and `topmatic reset` starts clean
- A timer is only touched if it carries topmatic's schedule drop-in (`topmatic@<profile>.timer.d/10-schedule.conf`). Timers from other tools, even ones named `topmatic@*`, are left alone with a note

## Requirements

- Linux with systemd (user session)
- [topgrade](https://github.com/topgrade-rs/topgrade) in `PATH` (`cargo install topgrade`, or your distro package manager)
- Rust toolchain (to install from source)

## Install

```sh
cargo install --git https://github.com/c4i0b/topmatic
```

Then run `topmatic` once. It installs the systemd user units and converges the timers to your config. To let timers fire while you are logged out enable lingering yourself (`loginctl enable-linger`). topmatic shows its state and explains it but never runs anything privileged.

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

Presets work as overlays. Picking one in the TUI activates it immediately under its suggested name, no name to type and no editor to cross. `All` runs everything installed with no step list and picks up steps topgrade gains later. Editing a preset profile works like any other but the config keeps only what you changed. `extra_steps` and `excluded_steps` adjust the selection and a reverted override disappears from the file again.

Keys per screen (the footer always shows them). Home: `↑↓ move`, `n` new, `e` edit, `d` delete, `r` run now (opens the live log right away, `x` stops the run, `esc` goes back), `l` logs, `/` filter, `?` help, `q` quit. Editor: `↑↓←→ move`, `enter` toggle steps or choose a row, `ctrl+a` all, `ctrl+d` none, `tab` section, `ctrl+s` save from anywhere, `esc` back (it asks to save when there are changes). Presets: `enter` choose. Logs: `enter` open, `r` refresh, `esc` back. Mouse click and scroll work. `L` toggles the activity panel. The right pane shows schedule, countdown, timer state and the tail of the last run.

The editor cycles its sections with `Tab`: steps, schedule, options. `Enter` toggles a step or opens a picker for the schedule frequency or notifications. Schedules are frequencies anchored at midnight (daily, every 6 or 12 hours, weekly, every 2 weeks or monthly) and missed runs catch up on the next boot.

Custom profiles start from the picker's custom entry and select steps from the live topgrade catalog, so the grid always matches what is installed.

![editor](docs/assets/editor.png)

Press `?` anywhere for the full key map and what lingering means:

![help](docs/assets/help.png)

Every run leaves a log you can browse with `l`:

![logs](docs/assets/logs.png)

Profiles live in `~/.config/topmatic/config.toml`. Edit it by hand or via TUI, both are first class. topmatic reconciles systemd to match it on every start. A `[defaults]` table tunes run behavior for every profile (`retries`, `retry_delay`, `give_up_after`, `network_wait`, `random_delay` as `2min`-style values). `config.example.toml`, regenerated next to it, documents every key.

### Schedules as dotfiles

`~/.config/topmatic/config.toml` is the single source of truth and safe to share between machines, for example into your dotfiles repo. On a new machine restore that file and run `topmatic` once. It installs the unit templates and recreates every timer. Do **not** share `~/.local/state/topmatic` (run history and logs are machine local) or `~/.config/systemd/user`. Those units regenerate automatically and stale symlinks are repaired by the sync.

Two safety guarantees around the sync:

- A timer without a matching profile but **with** topmatic's schedule drop-in is an orphan of a removed profile and gets cleaned up.
- A `topmatic@*` timer **without** topmatic's drop-in was created by something else. topmatic leaves it alone and just reports it so other tools can coexist.

## Development

```sh
just check # fmt + clippy + tests
```

`just --list` shows every recipe. Tests use stubs instead of real topgrade/systemd so they run anywhere a Rust toolchain exists. The only exception is `tests/systemd_integration.rs`, which needs a real user systemd and skips otherwise.

## License

MIT
