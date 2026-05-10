# kameo-testing — AGENTS

This repository is a workspace-owned testing bed. It feeds
`~/primary/skills/kameo.md` and is referenced by every report or
skill that makes claims about Kameo's behavior.

Read `~/primary/AGENTS.md` for the workspace contract. The
load-bearing per-repo rules:

- **Tests are the contract.** Every claim in `~/primary/skills/kameo.md`
  cites a test in `tests/`. If a behavior is asserted in prose but
  not under a green `nix flake check`, the prose is wrong until proven
  otherwise.

- **Notes carry sources.** `notes/<subsystem>.md` files are the
  research substrate. Every non-obvious claim in a note carries a
  source URL or `github.com/tqwewe/kameo` source path.

- **No mocks.** Kameo behavior under test is real Kameo, not a fake.

- **Discoveries land in `notes/findings.md`.** Anything that surprised
  the test author — an undocumented behavior, a footgun, a
  divergence from intuition — gets a one-paragraph entry there. The
  Kameo skill draws from this file for its anti-pattern section.
