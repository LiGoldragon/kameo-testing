# kameo-testing

Kameo 0.20 testing bed. The repository is the falsifiable source for
the workspace's Kameo skill (`primary/skills/kameo.md`): every
substantive claim in the skill corresponds to a passing test here.

## Layout

```
notes/    — Kameo research notes (sourced from docs.rs + github)
src/      — shared test utilities (minimal)
tests/    — integration tests, one file per Kameo subsystem
flake.nix — crane + fenix; `nix flake check` runs everything
```

## Run

```sh
nix flake check
```

## Why this exists

Kameo became the workspace's actor runtime default after the
`persona-actor` / `workspace-actor` hallucination thread (operator/103).
This repo is the evidence base for that decision and the working
surface for the skill that captures it.
