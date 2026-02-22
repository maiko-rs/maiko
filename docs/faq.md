# Frequently Asked Questions

## What is Maiko?

Maiko is a lightweight actor framework for Tokio. It helps you structure applications as independent components (actors) that communicate purely through events.

Each actor:
- Has its own state and logic
- Runs in its own async task
- Receives events through a private mailbox
- Publishes events without knowing who receives them

Maiko handles all the routing. Actors subscribe to topics, and when an event is published, it's automatically delivered to interested subscribers. No manual channel wiring needed.

## How does Maiko relate to other actor frameworks?

Maiko borrows the core idea from the actor model: isolated units with private state, communicating through messages.

**Key difference from most actor frameworks**: Maiko uses topic-based routing instead of direct addressing. You publish an event *about* something; you don't send a message *to* a specific actor. Publishers don't need to know who's listening.

**What Maiko is not**:
- Not distributed (single process, single Tokio runtime)
- Not a supervision tree (single flat supervisor, restart strategies planned)
- Not request-response (fire-and-forget events only)

For detailed comparisons, use cases, and when to choose alternatives, see **[Why Maiko?](why-maiko.md)**.

## How stable is Maiko?

**Current status**: Early-stage, API stabilizing.

| Version | Focus |
|---------|-------|
| 0.1.0 | Core actor model, basic routing |
| 0.2.0 | Monitoring, test harness, API refinements |
| 0.2.5 | Backpressure, per-actor configuration, overflow policies |
| 0.3.0 (planned) | Supervision, dynamic actors, improved error handling |
| 1.0.0 (future) | API stability guarantee |

**What this means**:
- Suitable for side projects, prototypes, and non-critical systems
- Used in production by Charon (single real-world deployment)
- API may change between minor versions
- Not recommended for mission-critical production systems yet

**Known limitations**:
- No dynamic actor registration after startup (planned)
- Limited error recovery (actors crash on unhandled errors)
- Single broker (scaling to multiple brokers planned)

## How do I test a Maiko application?

Maiko includes a **test harness** (built on the monitoring API) designed for testing actor systems. Enable it with `features = ["test-harness"]`.

The harness lets you:
- **Record events** during a test window
- **Inject events** as if sent by specific actors
- **Query event flow** - who sent what to whom
- **Assert on delivery** - verify events reached the right actors

```rust
#[tokio::test]
async fn test_temperature_alert() -> Result<()> {
    let mut sup = Supervisor::<WeatherEvent, WeatherTopic>::default();

    let sensor = sup.add_actor("sensor", |ctx| TempSensor::new(ctx), Subscribe::none())?;
    let alert = sup.add_actor("alert", |ctx| AlertMonitor::new(ctx), &[WeatherTopic::Reading])?;

    let mut harness = Harness::new(&mut sup).await;
    sup.start().await?;

    // Record what happens when we inject a high temperature reading
    harness.start_recording().await;
    let event_id = harness.send_as(&sensor, WeatherEvent::Temperature(45.0)).await?;
    harness.stop_recording().await;

    // Verify the alert actor received the reading
    assert!(harness.event(event_id).was_delivered_to(&alert));

    // Verify an alert was generated
    let alerts = harness.events().with_topic(&WeatherTopic::Alert);
    assert_eq!(alerts.count(), 1);

    sup.stop().await
}
```

The harness provides spies for inspecting actors, events, and topics:
- `harness.actor(&id)` - what did this actor send/receive?
- `harness.event(id)` - where did this event go?
- `harness.events()` - query all recorded events with filters

See the **[Test Harness Documentation](testing.md)** for the full API.

## Can I add or remove actors at runtime?

Not currently. All actors must be registered before calling `supervisor.start()`. This is a known limitation.

Dynamic actor registration is on the roadmap. Use cases like "spawn an actor per WebSocket connection" aren't supported yet.

## How does error handling work?

Currently minimal. If an actor's `handle_event` returns an error:
1. The `on_error` hook is called (you can override this to recover)
2. By default, the error propagates and the actor stops

Proper supervision (restart strategies, escalation policies) is planned for 0.3.0.

## Why is there no `try_send` in Context?

By design. `ctx.send().await` is the only way to send events, and it waits for the broker to have capacity.

**The tradeoff:**

Pure actor independence says: fire and forget, don't care what happens downstream.

`ctx.send().await` says: wait if the broker's channel is full.

These conflict. Maiko chooses bounded channels with backpressure. When the broker can't accept more events, producers wait. This couples senders to system capacity - not to specific receivers, but to the broker's ability to accept.

**Why this choice?**

Pure fire-and-forget needs somewhere for events to go:
- Unbounded queues → memory exhaustion under load
- Bounded queues + silent drop → data loss, hard to debug
- Bounded queues + wait → stable system, explicit behavior

The "independence" in actor model means: no shared memory, no knowledge of specific recipients, asynchronous communication. It doesn't mean "producers never wait."

**Why not offer both?**

A `try_send` would let producers bypass backpressure:
```rust
// Hypothetical - doesn't exist
ctx.try_send(event)?;  // Returns immediately, drops if broker busy
```

Problems:
- **Incoherent model**: Receiver configured to block, but producer doesn't wait. Mixed signals.
- **Hidden data loss**: Events silently dropped, hard to debug
- **Two ways to send**: Users must understand when to use which

**If you truly need fire-and-forget:**
```rust
let ctx = ctx.clone();
tokio::spawn(async move {
    let _ = ctx.send(event).await;
});
```

Explicit. Visible. Your choice. Not a blessed API that muddles the model.

**Philosophy**: One way to send. Backpressure by default. Honest about the tradeoff.

## What about performance?

Maiko is designed for correctness and simplicity first. That said:

- **Event routing**: O(N) over subscribers, filtered by topic. Fast for realistic actor counts (tens to low hundreds).
- **Memory**: Events are `Arc<Envelope<E>>` - zero-copy fan-out to multiple subscribers.
- **Overhead**: One `mpsc` channel per actor, one broker task doing the routing.

For most event-driven applications, Maiko won't be the bottleneck. If you need bare-metal performance with zero allocations, consider Stakker.

## Does Maiko support distributed systems?

Maiko is in-process only - all actors run within a single Tokio runtime. There's no built-in location transparency or actor clustering.

However, events are serializable. Enable the `serde` feature and your events can be sent over the wire. You can build bridge actors that forward events to external systems via IPC, WebSockets, or message queues.

**Real-world example**: Charon uses a bridge actor to expose Maiko events over a Unix socket, allowing external processes to communicate with the daemon.

For true distributed actors across machines with automatic routing, consider Ractor.

## What does "Maiko" mean?

Maiko (舞妓) are apprentice geisha in Kyoto, known for their coordinated traditional dances. Like maiko performers who respond to music and each other in harmony, Maiko actors coordinate through events in the Tokio runtime.

## Can I contribute?

Yes, absolutely! Contributions of all kinds are welcome - bug reports, feature ideas, documentation improvements, or code.

**Ways to get involved**:
- **Try it out** and [share your experience](https://github.com/maiko-rs/maiko/discussions/41)
- **Report issues** or suggest improvements via [GitHub Issues](https://github.com/maiko-rs/maiko/issues)
- **Pick up a task** from [good first issues](https://github.com/maiko-rs/maiko/issues?q=is%3Aissue+state%3Aopen+label%3A%22good+first+issue%22)
- **Ask questions** - if something's unclear, that's a documentation bug
- Check [CONTRIBUTING.md](https://github.com/maiko-rs/maiko/blob/main/CONTRIBUTING.md) for details

Maiko is a young project and your input genuinely shapes its direction. Whether you're fixing a typo or proposing a major feature, it's appreciated.
