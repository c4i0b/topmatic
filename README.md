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

## Requirements

- Linux with systemd (user session)
- [topgrade](https://github.com/topgrade-rs/topgrade) in `PATH`
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
topmatic sync     # converge systemd units to the config
topmatic edit     # edit the config with $EDITOR, then sync
topmatic run <profile> [--dry-run]
```

In the TUI: `n` new, `e` edit, `space` pause/resume, `d` delete, `r` run now, `t` dry-run test, `l` logs, `?` help.

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
