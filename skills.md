# kameo-testing — skills

Repo-specific guidance for editing this repository.

## What this repo is for

A test surface that proves what Kameo actually does. Tests live and
die on `nix flake check`. The workspace's `~/primary/skills/kameo.md`
draws its examples and prose from passing tests here.

## When you edit a test

- The test name is the claim. `it_works` is forbidden; the name spells
  out the falsifiable assertion.
- One concern per test. If a test asserts two things, split.
- No `unwrap` in test bodies hiding intermediate errors. Every
  expected failure is asserted on type and content.

## When you add a new subsystem

1. Add a file `tests/<subsystem>.rs`.
2. Update `ARCHITECTURE.md` §"Code map" with the new file.
3. Add a `notes/<subsystem>.md` if the subsystem isn't covered by an
   existing notes file.
4. Cross-reference from `~/primary/skills/kameo.md` once it lands.

## Discoveries

When a test surfaces something surprising, append a one-paragraph
entry to `notes/findings.md`. The skill draws its anti-patterns from
that file.

## See also

- `~/primary/skills/skill-editor.md` — skill conventions.
- `~/primary/skills/rust-discipline.md` — Rust style; tests live in
  `tests/`, not in `#[cfg(test)] mod tests`.
- `~/primary/skills/architectural-truth-tests.md` — the witness
  pattern (this repo is itself a witness for the skill's claims).
