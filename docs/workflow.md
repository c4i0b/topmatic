# Workflow

Task tracking with beads. bd runs on the HOST, never in the container.

- `bd ready` to find available work; `bd update <id> --claim` to claim;
  `bd close <id>` to complete. `bd prime` for full context at session
  start.
- Persistent decisions go in `bd remember`, never in markdown memory
  files.
- `bd remember` example: `shot-container-bakeoff` holds the vhs screenshot
  container bakeoff details referenced by [architecture.md](architecture.md).
- Never commit, push or `bd dolt push` unless explicitly asked. Beads data
  syncs through the Dolt remote, not git; `.beads/issues.jsonl` is a
  passive export.
