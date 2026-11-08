# docs/ index

| Doc | Trust | What it is / when to consult it |
|---|---|---|
| [environment.md](environment.md) | live | How commands run: the ephemeral tooling container, recipes, the live systemd bridge. Consult before touching justfile/Containerfile/compose. |
| [development.md](development.md) | live | Day-to-day coding discipline. Consult before writing code or tests. |
| [testing.md](testing.md) | live | Test layers, offline default, live opt-in, fixture policy. Consult before adding or moving tests. |
| [architecture.md](architecture.md) | live | Domain layer rules, registries, timestamps, systemd quirks. Consult before refactoring src/. |
| [workflow.md](workflow.md) | live | Task tracking with beads. Consult when picking up or closing work. |
| [aur.md](aur.md) | live | AUR packaging: prepared PKGBUILD, pending account, release-driven auto-update. Consult before touching packaging/aur or the aur workflow. |
| [nix.md](nix.md) | live | Nix flake: cargoHash rules, nixbox distrobox, release interplay across AUR/nixpkgs/flake. Consult before releases that change dependencies. |
| assets/ | reference | Screenshots used by the README, regenerated manually (see architecture.md). |
