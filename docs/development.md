# Development policy

1. **Test-first, always.** The test is written or adjusted before or with
  the implementation. Code never lands bare.
2. **Cheapest layer first.** Decide which test layer sees the behavior
  (see [testing.md](testing.md)) and write it there and only there.
3. **Refactor for testability.** If a behavior can only be verified at an
  expensive layer (real systemd, real topgrade, live network), refactor
  first: seams, injection, stubs in tempdirs. Modularity is the
  consequence.
4. **Warnings at root cause.** Fix the warning, not the symptom.
5. **Explicit over implicit.** Declare effects at the boundary. Validate
  degenerate inputs before third-party math.
6. **Dead code is deleted**, never commented out.
7. Code, commits and messages in English. No comments in source:
  `#[command(about)]` for clap help, doc rules live in docs/.
8. Fixtures are captured from real tool output, never invented. A
  fabricated timestamp format once hid a real bug.

Worked example for rule 3: the systemd live test used to write a unit file
into `$HOME/.config/systemd/user`, which broke the moment tests ran in a
container (HOME=/tmp is invisible to the host manager). Refactored to a
`systemd-run --user` transient unit, which needs no filesystem path at all.
