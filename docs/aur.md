# AUR packaging (prepared, not yet submitted)

`packaging/aur/PKGBUILD` builds topmatic from the tagged GitHub source
tarball with `cargo fetch --locked` + `cargo build --frozen`. It was
validated in a clean `archlinux:base-devel` container (build, tests and
namcap). No systemd units are packaged: topmatic generates its own user
units at runtime.

`topgrade` is an AUR-only dependency (not in the official repos), which
is fine: AUR helpers (paru/yay) resolve AUR-to-AUR depends.

## Pending: AUR account

Registration at https://aur.archlinux.org/register is temporarily closed
(automated-signup wave). When it reopens (announced on aur-general and
the Arch news feed, do not script retries):

1. Register, add an SSH public key in Account Settings.
2. `git clone ssh://aur@aur.archlinux.org/topmatic.git`
3. Copy `packaging/aur/PKGBUILD` in, run
   `makepkg --printsrcinfo > .SRCINFO`, commit both, push to `master`.

## Auto-update

`.github/workflows/aur.yml` triggers on `release: published` but is
**disabled by default**: the job is gated on the repository variable
`AUR_ENABLED == 'true'`. When the account exists and the secrets below
are set, enable it in Settings → Secrets and variables → Actions →
Variables (no code change needed). It clones the AUR repo fresh, bumps
only `pkgver`/`pkgrel`/`sha256sums` (preserving co-maintainer edits),
regenerates `.SRCINFO` and pushes without force. Requires repo secrets
`AUR_SSH_PRIVATE_KEY` (dedicated CI key, public half also registered in
the AUR account) and `AUR_KNOWN_HOSTS`. The workflow only updates an
existing package — the first submission above is manual.
