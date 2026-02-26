# Test Harness

Maiko provides a test harness for observing and asserting on event flow. Enable it with the `test-harness` feature:

```toml
[dev-dependencies]
maiko = { version = "0.3", features = ["test-harness"] }
```

## Overview

The test harness enables:
- **Event recording** - capture all event deliveries during a test
- **Event injection** - send events as if they came from specific actors
- **Condition-based settling** - wait until specific events appear
- **Spies** - inspect events from different perspectives (event, actor, topic)
- **Queries** - filter and search recorded events
- **Event chains** - trace event propagation through parent IDs

## Basic Usage

```rust
#[tokio::test]
async fn test_event_flow() -> Result {
    let mut sup = Supervisor::<MyEvent>::default();
    let producer = sup.add_actor("producer", |ctx| Producer::new(ctx), Subscribe::all())?;
    let consumer = sup.add_actor("consumer", |ctx| Consumer::new(ctx), Subscribe::all())?;

    // Initialize harness BEFORE starting
    let mut test = Harness::new(&mut sup).await;
    sup.start().await?;

    // Record, send, settle
    test.record().await;
    let id = test.send_as(&producer, MyEvent::Data(42)).await?;
    test.settle().await;

    // Assert on recorded events
    assert!(test.event(id).was_delivered_to(&consumer));
    assert_eq!(1, test.actor(&consumer).events_received());

    sup.stop().await
}
```

## Recording Control

```rust
// Start recording events
test.record().await;

// ... send events, run test scenario ...

// Settle (drain events until quiet, then freeze snapshot)
test.settle().await;

// Clear recorded events for next test phase
test.reset();
```

## Event Injection

Send events as if they came from a specific actor:

```rust
let event_id = test.send_as(&producer, MyEvent::Data(42)).await?;
```

The returned `event_id` can be used to inspect the event's delivery.

## Settling

### settle() - silence-based

Drains events until the system is quiet (no new events for 1ms, or 10ms total). Use for simple send-and-check tests:

```rust
test.record().await;
test.send_as(&producer, MyEvent::Trigger).await?;
test.settle().await;
```

### settle_on() - condition-based

Waits until a predicate is satisfied. Returns `Error::SettleTimeout` if the condition isn't met within the timeout (default 1 second):

```rust
// Wait until 5 HidReport events are recorded
test.settle_on(|events| events.with_label("HidReport").count() >= 5).await?;

// With a custom timeout
test.settle_on(|events| events.with_label("Pong").exists())
    .within(Duration::from_secs(3))
    .await?;

// Filter chains with exists()
test.settle_on(|events| events.sent_by(&alice).with_label("Ping").exists()).await?;
```

### settle_on_event() - wait for a specific event

Shorthand for the most common case - waiting for a specific event to appear:

```rust
// By label (requires Event: Label)
test.settle_on_event("Pong").await?;

// By matcher
test.settle_on_event(EventMatcher::by_event(|e| matches!(e, MyEvent::Pong))).await?;

// With timeout
test.settle_on_event("Pong").within(Duration::from_secs(3)).await?;
```

## Spies

Spies provide focused views into recorded events.

### EventSpy

Inspect a specific event by ID:

```rust
let spy = test.event(event_id);

spy.was_delivered()              // true if delivered to any actor
spy.was_delivered_to(&consumer)  // true if delivered to specific actor
spy.not_delivered_to(&consumer)  // true if NOT delivered to specific actor
spy.was_delivered_to_all(&[&a, &b])  // true if delivered to all listed actors
spy.delivery_ratio(&[&a, &b, &c])   // fraction of listed actors that received it
spy.sender()                     // name of sending actor
spy.receivers()                  // list of receiving actors
spy.receiver_count()            // number of receivers
spy.children()                   // query for child events
```

### ActorSpy

Inspect events from an actor's perspective:

```rust
let spy = test.actor(&consumer);

// Inbound (events received)
spy.inbound()          // EventQuery of received events
spy.events_received()  // number of events received
spy.last_received()    // most recent received event
spy.received_from()    // actors that sent events to this actor
spy.sender_count()     // count of distinct senders

// Outbound (events sent)
spy.outbound()         // EventQuery of sent events
spy.events_sent()      // number of distinct events sent
spy.last_sent()        // most recent sent event
spy.sent_to()          // actors that received events from this actor
spy.receiver_count()   // count of distinct receivers
```

### TopicSpy

Inspect events on a specific topic:

```rust
let spy = test.topic(MyTopic::Data);

spy.was_published()    // true if any events on this topic
spy.event_count()      // number of event deliveries
spy.receivers()        // actors that received events on this topic
spy.receiver_count()  // count of distinct receivers
spy.events()           // EventQuery for further filtering
```

## EventQuery

`EventQuery` provides a fluent API for filtering recorded events:

```rust
// Get all recorded events
let query = test.events();

// Chain filters
let orders = test.events()
    .sent_by(&trader)
    .received_by(&exchange)
    .matching_event(|e| matches!(e, MyEvent::Order(_)))
    .count();

// Filter operations (chainable, return Self)
query.sent_by(&actor)           // events sent by actor
query.received_by(&actor)       // events received by actor
query.with_topic(topic)         // events on specific topic
query.with_id(event_id)         // events with specific ID
query.children_of(id)           // child events of a parent event
query.with_label("MyVariant")   // events with specific label (requires Label trait)
query.matching_event(|e| ...)   // custom event predicate
query.matching(|entry| ...)     // custom entry predicate (access to metadata)
query.after(&entry)             // events after a given event (by timestamp)
query.before(&entry)            // events before a given event (by timestamp)

// Terminal operations
query.count()           // number of matching events
query.is_empty()        // true if no matches
query.exists()          // true if any matches
query.first()           // first matching event
query.last()            // last matching event
query.nth(n)            // nth matching event
query.collect()         // unique events (deduplicated by event ID)
query.all_deliveries()  // all delivery records (including duplicates)
query.senders()         // unique sender actor IDs
query.receivers()       // unique receiver actor IDs
query.count_by_label()  // HashMap<String, usize> of event counts per label
query.all(|entry| ...)  // true if all matching events satisfy predicate
query.any(|entry| ...)  // true if any matching event satisfies predicate

// Boolean convenience methods
query.has_event("Ping")         // true if any event matches (by label, ID, or matcher)
query.has_sender(&actor)        // true if any event was sent by actor
query.has_receiver(&actor)      // true if any event was received by actor
query.has(|entry| ...)          // true if any event satisfies predicate
```

Queries can be chained from spies:

```rust
// Events sent by normalizer that were received by trader
let events = test.actor(&normalizer)
    .outbound()
    .received_by(&trader)
    .count();
```

## EventMatcher

`EventMatcher` identifies a single event by criteria. Used with `settle_on_event()`, `has_event()`, and chain queries:

```rust
// Factory methods
EventMatcher::by_label("KeyPress")                              // by label
EventMatcher::by_id(event_id)                                   // by event ID
EventMatcher::by_event(|e| matches!(e, MyEvent::KeyPress(_)))   // by event payload
EventMatcher::by_entry(|entry| entry.sender() == "scanner")     // by entry (full metadata)

// Into<EventMatcher> conversions (used automatically)
"KeyPress"    // &str → label matcher (requires E: Label)
event_id      // EventId → id matcher
```

## Event Chains

`EventChain` traces causally related events through parent IDs, building a tree from a root event to all its descendants.

```rust
let chain = test.chain(root_event_id);
```

### Actor Tracing

Query which actors were visited and in what order:

```rust
// All actors involved (any branch)
chain.actors().all();

// Verify an exact root-to-leaf path exists
chain.actors().exact(&[&scanner, &pipeline, &writer, &telemetry]);

// Verify a contiguous sub-path within any branch
chain.actors().segment(&[&pipeline, &writer]);

// Verify reachability with gaps (any branch)
chain.actors().passes_through(&[&scanner, &telemetry]);

// Count distinct paths
chain.actors().path_count();
```

### Event Tracing

Query the event sequence along parent-child paths:

```rust
// Check if a specific event label appears anywhere
chain.events().contains("HidReport");

// Verify an exact event path (root to leaf)
chain.events().exact(&["KeyPress", "HidReport", "ReportSent"]);

// Verify a contiguous segment within any branch
chain.events().segment(&["KeyPress", "HidReport"]);

// Verify ordering with gaps (any branch)
chain.events().passes_through(&["KeyPress", "ReportSent"]);

// Count distinct event paths
chain.events().path_count();
```

For branching chains (one event triggering multiple children), `exact`, `segment`, and `passes_through` check each branch independently.

### Branching

Inspect fan-out patterns:

```rust
chain.diverges_after("KeyPress");     // true if multiple children
chain.branches_after("KeyPress");     // number of child events
chain.path_to(&telemetry);            // sub-chain to a specific actor
```

### Debug Output

```rust
// Tree view
chain.pretty_print();
// EventChain (root: 123...)
// KeyPress [KeyScanner -> KeyEventPipeline, Telemetry]
// └─ HidReport [KeyEventPipeline -> KeyWriter, Telemetry]
//    └─ ReportSent [KeyWriter -> Telemetry]

// Mermaid sequence diagram
let diagram = chain.to_mermaid();
// sequenceDiagram
//     KeyScanner->>KeyEventPipeline:KeyPress
//     KeyScanner->>Telemetry:KeyPress
//     KeyEventPipeline->>KeyWriter:HidReport
//     KeyEventPipeline->>Telemetry:HidReport
//     KeyWriter->>Telemetry:ReportSent
```

## Debugging

Dump all recorded events for debugging:

```rust
test.dump();
// Output:
// Recorded events (3 deliveries):
//   0: [Producer] --> [Consumer]  (id: 123...)
//   1: [Consumer] --> [Logger]    (id: 456...)
//   2: [Consumer] --> [Database]  (id: 456...)
```

Get event count:

```rust
let count = test.event_count();
```

## Example: Testing Event Cascades

```rust
#[tokio::test]
async fn test_order_processing_pipeline() -> Result {
    let mut sup = Supervisor::<OrderEvent, OrderTopic>::default();

    let gateway = sup.add_actor("gateway", |ctx| Gateway::new(ctx), Subscribe::to(&[OrderTopic::Incoming]))?;
    let validator = sup.add_actor("validator", |ctx| Validator::new(ctx), Subscribe::to(&[OrderTopic::Incoming]))?;
    let processor = sup.add_actor("processor", |ctx| Processor::new(ctx), Subscribe::to(&[OrderTopic::Validated]))?;
    let notifier = sup.add_actor("notifier", |ctx| Notifier::new(ctx), Subscribe::to(&[OrderTopic::Processed]))?;

    let mut test = Harness::new(&mut sup).await;
    sup.start().await?;

    test.record().await;
    test.send_as(&gateway, OrderEvent::NewOrder(order)).await?;

    // Wait for the notifier to receive the processed order
    test.settle_on(|events| events.received_by(&notifier).exists()).await?;

    // Verify pipeline
    assert_eq!(1, test.actor(&validator).events_received());
    assert_eq!(1, test.actor(&processor).events_received());
    assert_eq!(1, test.actor(&notifier).events_received());

    // Verify event flow
    assert!(test.topic(OrderTopic::Incoming).was_published());
    assert!(test.topic(OrderTopic::Validated).was_published());
    assert!(test.topic(OrderTopic::Processed).was_published());

    sup.stop().await
}
```

## Complete Example

See [`examples/arbitrage.rs`](../maiko/examples/arbitrage.rs) for a comprehensive demonstration of the test harness, including all spy types, queries, and assertion patterns.

## Limitations

- **Async timing**: `settle()` waits for actors to receive events, but not necessarily for them to finish processing. For long-running handlers, use `settle_on()` with a condition that checks for expected output.
- **Recording overhead**: When the test harness is enabled, there's minimal overhead even when not actively recording.
- **Single supervisor**: The harness is tied to a single supervisor instance.

## Performance Considerations

> **The test harness is designed for testing only. Do not use it in production.**

### Why Not Production?

The test harness uses an **unbounded channel** to collect events. This design choice prioritizes correctness and simplicity for testing:

- Events are never dropped, ensuring test assertions are reliable
- No backpressure that could affect actor timing during tests
- Simple implementation without complex flow control

However, in production this means:

- **Unbounded memory growth** - A fast producer with a slow consumer will accumulate events indefinitely
- **No backpressure** - The system won't slow down when overwhelmed
- **Memory exhaustion risk** - Long-running systems can run out of memory

### For Production Monitoring

If you need production observability, use the [monitoring API](monitoring.md) directly with a custom `Monitor` implementation that:

- Uses bounded channels or ring buffers
- Samples events under high load
- Batches writes to external systems
- Handles backpressure appropriately

### Settle Timing

The `settle()` method uses a timeout-based approach:

```rust
pub const DEFAULT_SETTLE_WINDOW: Duration = Duration::from_millis(1);
pub const DEFAULT_MAX_SETTLE: Duration = Duration::from_millis(10);
```

- **Settle window** (1ms): Returns when no events arrive for this duration
- **Max settle** (10ms): Maximum time to wait, even if events keep arriving

For condition-based settling, use `settle_on()` which defaults to a 1-second timeout, overridable with `.within()`.
