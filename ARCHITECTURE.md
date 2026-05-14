# kameo-testing — architecture

*Falsifiable source for the workspace's Kameo skill.*

## Role

Capture Kameo 0.20's actual behavior under test, in a form the
workspace can cite. Not a library. Not a daemon. A reference repo.

## Code map

```
notes/                              — research substrate
├── lifecycle-and-messages.md         Actor / Message / Reply / Spawn
├── supervision-and-mailbox.md        RestartPolicy / SupervisionStrategy / Mailbox
├── registry-streams-remote.md        Registry / Streams / Remote / misc
└── findings.md                       Surprises, footguns, anti-patterns

src/lib.rs                          — minimal; shared test fixtures only
tests/
├── lifecycle.rs                    — on_start, on_stop, on_panic, ControlFlow
├── messages.rs                     — Message<T> impls, Reply, DelegatedReply
├── supervision.rs                  — RestartPolicy × SupervisionStrategy
├── mailbox.rs                      — bounded backpressure, unbounded, ask vs tell
├── spawn.rs                        — spawn, spawn_with_mailbox, prepare/run, spawn_in_thread
├── registry.rs                     — register / lookup / collisions
├── streams.rs                      — attach_stream, StreamMessage envelopes
├── links.rs                        — link / unlink, on_link_died
└── topology.rs                     — workspace witness: no-public-ZST-actor compile-fail
```

## Invariants

- Every test name reads as a falsifiable claim about Kameo's behavior
  (e.g. `on_start_failure_propagates_to_parent_supervisor`).
- `nix flake check` is the canonical runner. `cargo test` is fine
  during the inner loop but isn't the gate.
- Test files have no internal `mod tests` blocks — they ARE the tests.

## See also

- `~/primary/skills/kameo.md` — the skill this repo backs.
- `~/primary/skills/actor-systems.md` — the actor discipline Kameo
  serves.
