# Topmatic

[![crates.io](https://img.shields.io/crates/v/topmatic.svg)](https://crates.io/crates/topmatic)

Your user-level tools updated automatically without sudo.

![dashboard](docs/assets/dashboard.png)

Flatpaks, cargo installs, npm globals, pipx apps.
[Topgrade](https://github.com/topgrade-rs/topgrade) already knows how to update all of them.

Topmatic turns Topgrade into scheduled jobs you manage from a TUI.

## Why

- Automatic updates of **your** software without root, daemons or cron hacks
- Your own Topgrade config stays untouched.
  Topmatic always runs Topgrade with an isolated config
- Missed runs catch up on the next boot.
  Flaky runs self-heal behind a connectivity check
- Jobs run at minimum priority so they never get in your way
- Automatic cleanup after every run: Topmatic clears what Topgrade leaves behind
- Every start repairs drift.
  `topmatic doctor` diagnoses and `topmatic reset` starts clean

## Requirements

- Linux with **systemd** (user session).
  Works out of the box on Fedora, Ubuntu, Debian, Arch and openSUSE,
  plus derivatives like Mint and Zorin (Ubuntu family) and Bazzite (Fedora Atomic).
  systemd is the only scheduler supported today
- [Topgrade](https://github.com/topgrade-rs/topgrade) in `PATH`
- Rust 1.85 or newer.
  Topmatic installs from source

## Install

Topmatic installs from source and needs Rust 1.85 or newer.

From the official repos where they ship a recent toolchain:

```sh
sudo dnf install rust cargo      # Fedora
sudo pacman -S rust              # Arch
sudo zypper install rust cargo   # openSUSE
```

Debian and Ubuntu carry Rust in their repos but usually too old for Topmatic.
Use rustup there (or anywhere):

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Then:

```sh
cargo install topmatic
```

Run `topmatic` once.
It installs the user units and converges the timers.

Topmatic refuses to run as root.
It manages your user's systemd session, so sudo would act on root's units instead.
Run it as your regular user.

To let timers fire while logged out enable lingering yourself:
`loginctl enable-linger`.
Topmatic never runs anything privileged.

## Use

```sh
topmatic          # TUI
topmatic list     # profiles, next run, last status
topmatic doctor   # diagnose and repair unit drift
topmatic sync     # converge systemd units to the config
topmatic run <profile> [--dry-run]
```

Every key is shown in the footer of each screen.
`?` opens the full map.

### Presets

- Picking a preset activates it immediately.
  No name to type, no editor to cross
- `All` runs everything installed with no step list.
  It picks up steps Topgrade gains later
- Editing a preset keeps only your changes in the config
  (`extra_steps`, `excluded_steps`, schedule).
  Reverted overrides disappear again

![editor](docs/assets/editor.png)

Every run leaves a log you can browse with `l`:

![logs](docs/assets/logs.png)

## Config

- Profiles live in `~/.config/topmatic/config.toml`.
  Edit it by hand or via TUI
- A `[defaults]` table tunes retries, retry delays, give-up time,
  network wait and jitter.
  `config.example.toml` documents every key
- The config is the single source of truth and safe to share between machines.
  Do **not** share `~/.local/state/topmatic` (machine-local history)
  or `~/.config/systemd/user` (regenerates)

Topmatic only touches timers that carry its schedule drop-in.
Timers from other tools, even ones named `topmatic@*`,
are left alone and just reported.

## Development

```sh
just check  # fmt + clippy + tests
```

`just --list` shows every recipe.
Tests use stubs so they run anywhere.
Exceptions: `tests/systemd_integration.rs` needs a real user systemd
and `just smoke` drives the real TUI in a pty.

## License

MIT
