# topmatic

Your user-level tools, updated automatically — without sudo.

![dashboard](docs/assets/dashboard.png)

Flatpaks, cargo installs, npm globals, pipx apps… [topgrade](https://github.com/topgrade-rs/topgrade) already knows how to update all of them. topmatic turns topgrade into scheduled jobs you manage from a TUI: pick what to update, when, and forget it.

## Why

- You want automatic updates of **your** software, without root, daemons or cron hacks
- Your own topgrade config stays untouched — topmatic always runs topgrade with an isolated config
- Machines are off sometimes: timers are persistent and catch up missed runs
- Jobs run at minimum priority (`Nice=19`, batch CPU, idle IO) — never in your way
- Default schedule spreads runs over the day (randomized), so nothing hammers mirrors at a privileged hour
- Self-healing: every start rewrites drifted unit files, removes orphan timers and prunes stray schedule overrides; `topmatic doctor` diagnoses and `topmatic reset` starts clean

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

In the TUI: `n` new, `e` edit, `space` pause/resume, `d` delete, `r` run now, `t` dry-run test, `l` logs, `/` filter, mouse click/scroll, `?` help. The right pane shows live details for the selected profile: schedule, countdown to next fire, timer state and the tail of the last run log.

Profiles can also track git repositories (bulk-imported by scanning a directory): each repo is pulled by the `git_repos` step and can run an `apply` command after (e.g. `stow` for dotfiles). Profiles with repos are dry-run verified on save and stay paused until the verification passes.

![editor](docs/assets/editor.png)

Profiles live in `~/.config/topmatic/config.toml` — edit it by hand or via TUI, both are first-class; topmatic reconciles systemd to match it on every start.

## Development

```sh
just check          # fmt + clippy + tests
just container-gate # same gate inside the devcontainer image
```

Tests run anywhere (stubs instead of real topgrade/systemd). Task runner is [just](https://github.com/casey/just); `just --list` shows everything, including how to regenerate the screenshots from `docs/assets/*.tape` with [vhs](https://github.com/charmbracelet/vhs).

## License

MIT
