# findings

*Surfaced from three research subagent passes against
`https://github.com/tqwewe/kameo/tree/v0.20.0/src/` and `docs.rs/kameo/0.20.0`.
Each fact is grounded in source; the exact path/line citations live in the
subagent transcripts (kameo `v0.20.0`, commit `2c075ec`).*

---

## Module map (where things live)

- `kameo::Actor` — re-exported from `kameo::actor::Actor`.
- `kameo::actor::{Spawn, ActorRef, WeakActorRef, ActorId, PreparedActor, Recipient, ReplyRecipient, RemoteActorRef}`.
- `kameo::error::{ActorStopReason, PanicError, PanicReason, SendError, HookError, Infallible, set_actor_error_hook}` — **all under `error`**, not `actor`.
- `kameo::mailbox::{bounded, unbounded, MailboxSender, MailboxReceiver, Signal}` — `bounded(n)` and `unbounded()` are **module-level free functions** that return `(tx, rx)` tuples; there is **no `Mailbox` type method**.
- `kameo::message::{Message, Context, StreamMessage, BoxMessage, BoxReply, DynMessage}`.
- `kameo::reply::{Reply, ReplyError, ReplySender, DelegatedReply, ForwardedReply, BoxReplySender}`.
- `kameo::request::{AskRequest, TellRequest, PendingReply, ...}`.
- `kameo::supervision::{RestartPolicy, SupervisionStrategy, SupervisedActorBuilder}`.
- `kameo::registry::ACTOR_REGISTRY` (only when `feature = "remote"` is **off**).
- `kameo::remote::*` (only when `feature = "remote"` is on); replaces local registry.

## Defaults

| Concern | Default |
|---|---|
| `RestartPolicy` | `Permanent` |
| `SupervisionStrategy` | `OneForOne` |
| `restart_limit` | 5 restarts per 5 seconds |
| Mailbox capacity | **64** (`pub(crate) const DEFAULT_MAILBOX_CAPACITY = 64` at `src/actor.rs:45`) |
| `on_panic` | `Break(Panicked(err))` — actor stops |
| `on_link_died` | `Continue` for `Normal`/`SupervisorRestart`, `Break(LinkDied{..})` otherwise |
| `on_stop` | `Ok(())` (no-op) |
| `Actor::Args` (when using `#[derive(Actor)]`) | `Self` |
| `Actor::Error` (when using `#[derive(Actor)]`) | `kameo::error::Infallible` (kameo's own, not `std::convert::Infallible`) |

## Reply mechanics — the load-bearing detail

`Reply` has `type Ok`, `type Error`, `type Value`. The caller's `ask().await`
returns `Result<R::Ok, SendError<M, R::Error>>`.

For `Reply = Result<T, E>` (the common fallible case): `Ok = T`, `Error = E`.

So:

- Handler returns `Ok(5_i64)` → caller's `ask().await` returns `Ok(5_i64)`.
- Handler returns `Err(MyError)` → caller's `ask().await` returns `Err(SendError::HandlerError(MyError))`.

A `tell` of a handler whose `Reply = Result<_, _>` and which returns `Err(_)`
becomes `ActorStopReason::Panicked(PanicError { reason: PanicReason::OnMessage })`.
The default `on_panic` stops the actor. **This is the `tell`-of-fallible-handler
trap**: a `Result::Err` from a `tell`'d handler crashes the actor unless
`on_panic` is overridden.

## Spawn shapes

- `Counter::spawn(Counter { ... })` — synchronous, returns `ActorRef<Counter>`.
- `Counter::spawn_with_mailbox(args, mailbox::bounded(n))` — synchronous, custom mailbox.
- `Counter::spawn_in_thread(args)` — synchronous, runs on dedicated OS thread; **panics on `current_thread` Tokio runtime** (use `#[tokio::test(flavor = "multi_thread")]`).
- `Counter::spawn_link(&peer_ref, args).await` — **async**, links to `peer_ref` before run loop starts.
- `Counter::supervise(&parent_ref, args).restart_policy(...).restart_limit(n, dur).spawn().await` — **async**, supervised. Requires `Args: Clone + Sync` for `supervise`; use `supervise_with(factory)` if not.
- `Counter::prepare()` returns `PreparedActor<Counter>` whose `actor_ref()` is available *before* `run(args).await` / `spawn(args)` / `spawn_in_thread(args)` is called — useful for pre-registering or pre-enqueueing.

## Lifecycle traps

- `on_start` returning `Err` does **not** call `on_stop`; lifecycle short-circuits to "stopped via on-start failure".
- `on_stop` panics are **not caught** in 0.20; will propagate as a tokio task panic.
- `on_stop` *errors* (returned `Err`) are stored in `shutdown_result` (visible via `wait_for_shutdown_result()`); the doc claim that they panic the task is stale.
- Self-`ask` from inside a handler **deadlocks** (the handler can't reply while occupied). Debug builds with tracing log a warning at the call site.
- Messages in the mailbox at the time of `kill()` are silently dropped; in-flight handler is aborted at the next `.await`.
- Restart-on-the-same-mailbox: messages queued at crash time **survive into the new instance** (the `MailboxReceiver` is recycled via `Signal::LinkDied`).

## Docs/source drift to know about

- `#[derive(Actor)] #[actor(mailbox = bounded(64))]` is documented but **not implemented**. Parser only accepts `name = "..."`. Use `spawn_with_mailbox` instead.
- Default mailbox capacity is **64**, not the 1000 the macro doc claims.
- Doc says `on_stop` errors panic the task — false in 0.20; they're stored in `shutdown_result`.
- `RpcReply` does not exist as a type. The closest concepts are `DelegatedReply`,
  `ForwardedReply`, `ReplySender`. References to "RpcReply" in older
  workspace docs are misnomers — likely confusion with ractor's `RpcReplyPort`.

## Tracing/metrics features

- `tracing` is on by default. Emits `actor.lifecycle` and `actor.handle_message`
  spans. Caller's `Span::current()` is captured and used as parent of the
  handler span (cross-actor traces nest).
- `metrics` (off by default) — `kameo_messages_sent`, `kameo_messages_received`,
  `kameo_lifecycle_*`, `kameo_link_died_*`, labelled by actor name.
- `otel` requires `tracing`; adds OpenTelemetry span links from handler span
  to actor lifecycle span.
- `hotpath` (new in 0.20, replaces `channels-console`) — channel saturation
  profiling per actor.

## Remote feature surface

When `feature = "remote"` is on:

- `kameo::registry` module **disappears**; `kameo::remote::*` replaces it.
- `ActorRef::register / lookup` switch from sync (returning local errors) to
  async (returning libp2p Kademlia errors).
- `RemoteActorRef<A>` for cross-process actor handles.
- Wire codec: libp2p `request_response::cbor` for envelopes, `rmp-serde`
  (MessagePack with named fields) for message bodies.
- TCP+QUIC+mDNS bootstrapped by default; WebSocket is **not** wired by
  `bootstrap()`.

For local-only systems, leave `remote` off. Persona-mind initially is
local-only; remoting is a deferred concern.
