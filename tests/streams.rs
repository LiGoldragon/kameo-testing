//! Stream tests — `ActorRef::attach_stream` and the `StreamMessage` envelope.

use kameo::error::Infallible;

use futures::stream;
use kameo::Actor;
use kameo::actor::{ActorRef, Spawn};
use kameo::message::{Context, Message, StreamMessage};

#[derive(Clone, Debug, PartialEq, Eq)]
enum Trace {
    Started(&'static str),
    Next(i64),
    Finished(&'static str),
}

// Trace lives on the actor itself — no shared lock. The actor is the
// owner; the test reads via `ask(ReadTrace)` which clones the inner
// Vec on demand. Per skills/actor-systems.md §"No shared locks":
// `Arc<Mutex<...>>` inside an actor is the gratuitous-shared-lock
// anti-pattern even when only the actor itself touches the lock —
// the lock is dead weight when the data already has a single owner.
struct Recorder {
    trace: Vec<Trace>,
}

impl Actor for Recorder {
    type Args = Self;
    type Error = Infallible;

    async fn on_start(args: Self, _ref: ActorRef<Self>) -> Result<Self, Self::Error> {
        Ok(args)
    }
}

impl Message<StreamMessage<i64, &'static str, &'static str>> for Recorder {
    type Reply = ();
    async fn handle(
        &mut self,
        msg: StreamMessage<i64, &'static str, &'static str>,
        _ctx: &mut Context<Self, Self::Reply>,
    ) {
        let trace = match msg {
            StreamMessage::Started(s) => Trace::Started(s),
            StreamMessage::Next(n) => Trace::Next(n),
            StreamMessage::Finished(f) => Trace::Finished(f),
        };
        self.trace.push(trace);
    }
}

struct ReadTrace;

impl Message<ReadTrace> for Recorder {
    type Reply = Vec<Trace>;
    async fn handle(&mut self, _msg: ReadTrace, _ctx: &mut Context<Self, Self::Reply>) -> Vec<Trace> {
        self.trace.clone()
    }
}

/// `attach_stream` delivers `Started`, then one `Next` per item, then
/// `Finished`. The actor sees them in order. After `handle.await`
/// returns, the Finished message is in the actor's mailbox; subsequent
/// `ask(ReadTrace)` is enqueued after Finished and the mailbox is FIFO,
/// so by the time `ReadTrace`'s handler runs the trace contains the
/// full sequence — no sleep needed.
#[tokio::test(flavor = "multi_thread")]
async fn attach_stream_delivers_started_next_finished_in_order() {
    let actor_ref = Recorder::spawn(Recorder { trace: Vec::new() });

    let stream = stream::iter([1_i64, 2, 3]);
    let handle = actor_ref.attach_stream(stream, "begin", "end");
    let _ = handle.await.expect("attach_stream task joins").expect("no SendError");

    let observed = actor_ref.ask(ReadTrace).await.expect("read trace ok");
    assert_eq!(
        observed,
        vec![
            Trace::Started("begin"),
            Trace::Next(1),
            Trace::Next(2),
            Trace::Next(3),
            Trace::Finished("end"),
        ]
    );
}

/// Empty stream still produces Started and Finished envelopes.
#[tokio::test(flavor = "multi_thread")]
async fn attach_stream_empty_still_emits_started_and_finished() {
    let actor_ref = Recorder::spawn(Recorder { trace: Vec::new() });

    let stream = stream::iter(Vec::<i64>::new());
    let handle = actor_ref.attach_stream(stream, "alpha", "omega");
    let _ = handle.await.expect("attach_stream task joins").expect("no SendError");

    let observed = actor_ref.ask(ReadTrace).await.expect("read trace ok");
    assert_eq!(
        observed,
        vec![Trace::Started("alpha"), Trace::Finished("omega")]
    );
}
