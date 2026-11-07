# Architecture

One source of truth per concern; registries derive, callers never hand-copy:

- UI keybindings live in `tui/bindings.rs` and feed footer, help and a
  compile-time README cross-check.
- Schedules/steps resolution lives in `domain/overlay.rs` and feeds argv,
  the runner, the dashboard and the editor.
- Persistence quirks (field omission, skip-if-empty) live at the
  serde/config edge, not sprinkled through business code.

The domain layer stays pure (no IO): resolvers, deltas, argv and timestamp
formatting are free functions with unit tests at the pyramid base. TUI
modules only dispatch and render.

## Timestamps

Store UTC, render through `util::format_in` with the machine zone. Tests
use `FixedOffset`, never env mutation (edition 2024 makes env mutation
unsafe in tests — use `Paths::with_bases`, never `set_var`).

## systemd quirks

- `Type=oneshot` units stay `activating` for the whole run: `is-active`
  exits 3, plain `start` blocks until completion. Detect via
  `show -p ActiveState`, start with `--no-block`, and treat
  `ExecMainStartTimestamp` as the run clock.
- Orphan cleanup only removes `topmatic@*` timers carrying topmatic's
  schedule drop-in (`topmatic@<p>.timer.d/10-schedule.conf`). Anything else
  named `topmatic@*` is foreign and must be left untouched, only reported.
- Production timers on the dev host are `topmatic@all-daily` and
  `topmatic@dev-daily`: never delete or disable them without asking.
- `topmatic sync`/TUI touch the real user systemd manager: dry-run first
  when verifying on the host.

## TUI verification

Host verification of TUI flows uses a python pty driver
(`scripts/pty_drive.py`: fork + TIOCSWINSZ + key injection). Diff frames
make per-phase substring checks flaky: assert on cumulative buffers, or
accept single-phase misses when unit tests already pin the behavior.

## Screenshots

README screenshots (`docs/assets/*.png`) are regenerated manually, never
via just: run `vhs docs/assets/*.tape` inside the `topmatic-shots`
distrobox (archlinux image), then extract the last frame with
`ffmpeg -i target/tapes/<x>.gif -vf reverse -frames:v 1` (vhs PNG `Output`
is unreliable). Recreate the box with
`distrobox create --name topmatic-shots --image docker.io/library/archlinux:latest`,
then `podman exec -u 0 topmatic-shots pacman -Sy --noconfirm vhs systemd
ttf-dejavu nss gtk3 alsa-lib` and copy `/usr/bin/topgrade` to
`~/.local/bin/` (topgrade is AUR-only).
