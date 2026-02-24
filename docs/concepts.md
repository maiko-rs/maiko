# Core Concepts

Maiko is built around a small set of core abstractions that work together:

- **Events** are messages that flow through the system
- **Topics** determine which actors receive which events (actors subscribe to topics)
- **Actors** are independent units that process events and maintain state
- **Context** allows actors to send events and interact with the runtime
- **Supervisor** manages actor lifecycles and coordinates the system
- **Envelopes** wrap events with metadata for tracing and correlation

This document covers each abstraction in detail.

## Events

Events are messages that flow through the system. They must implement the `Event` trait (there is derive macro for convenience):

```rust
#[derive(Event, Debug)]
enum NetworkEvent {
    PacketReceived(Vec<u8>),
    ConnectionClosed(u32),
    Error(String),
}
```

Events are:
- **Cloneable** - events may be delivered to multiple actors
- **Send + Sync** - as they travel between tasks and threads.

Events should be:
- **Debuggable** - for logging and diagnostics

## Topics

Topics group events and route them to interested actors. Each event maps to exactly one topic, determined by the `Topic::from_event()` implementation.

### Custom Topics

Define custom topics for fine-grained routing:

```rust
#[derive(Debug, Hash, Eq, PartialEq, Clone)]
enum NetworkTopic {
    Ingress,
    Egress,
    Control,
}

impl Topic<NetworkEvent> for NetworkTopic {
    fn from_event(event: &NetworkEvent) -> Self {
        match event {
            NetworkEvent::PacketReceived(_) => NetworkTopic::Ingress,
            NetworkEvent::ConnectionClosed(_) => NetworkTopic::Control,
            NetworkEvent::Error(_) => NetworkTopic::Control,
        }
    }
}
```

### Overflow Policy

Each topic defines what happens when a subscriber's channel is full, via `overflow_policy()`:

```rust
impl Topic<NetworkEvent> for NetworkTopic {
    fn from_event(event: &NetworkEvent) -> Self { /* ... */ }

    fn overflow_policy(&self) -> OverflowPolicy {
        match self {
            NetworkTopic::Ingress => OverflowPolicy::Block,   // wait for space
            NetworkTopic::Control => OverflowPolicy::Block,   // commands must arrive
            NetworkTopic::Egress  => OverflowPolicy::Drop,    // discard if slow
        }
    }
}
```

The default is `Fail` - the subscriber's channel is closed and the actor terminates. This surfaces problems immediately. See [`OverflowPolicy`](https://docs.rs/maiko/latest/maiko/enum.OverflowPolicy.html) for details.

### DefaultTopic

Use `DefaultTopic` when you don't need routing - all events go to all subscribed actors:

```rust
sup.add_actor("processor", factory, &[DefaultTopic])?;
```

## Actors

Actors are independent units that process or produce events. They implement the `Actor` trait with two core methods:

- **`handle_event`** - Process incoming events
- **`step`** - Produce events or perform periodic work

```rust
struct PacketProcessor {
    ctx: Context<NetworkEvent>,
    buffer: Vec<u8>,
}

impl Actor for PacketProcessor {
    type Event = NetworkEvent;

    async fn handle_event(&mut self, envelope: &Envelope<Self::Event>) -> Result<()> {
        match envelope.event() {
            NetworkEvent::PacketReceived(data) => {
                self.buffer.extend(data);
            }
            _ => {}
        }
        Ok(())
    }

    async fn step(&mut self) -> Result<StepAction> {
        if self.buffer.len() > 1000 {
            self.flush_buffer().await?;
        }
        Ok(StepAction::AwaitEvent)
    }
}
```

### Actor Roles

Concurrent tasks typically fall into one of three roles: **sources** that produce data, **sinks** that consume it, and **processors** that do both. Maiko doesn't have separate types for these - every actor implements the same `Actor` trait. The role emerges from which methods you use and what you subscribe to:

| Role | `step()` | `handle_event()` | Subscriptions |
|------|----------|-------------------|---------------|
| **Source** | Produces events | No-op | `Subscribe::none()` |
| **Sink** | `Never` (default) | Consumes events | Topics it cares about |
| **Processor** | Optional | Receives events, sends new ones | Selective topics |

A temperature sensor is a source - it uses `step()` to emit readings and subscribes to nothing. A logger is a sink - it handles events but never sends. An alerter is a processor - it receives readings and emits alerts.

This is a deliberate design choice. A single trait keeps the API small and lets actors evolve. A sink that later needs to emit events just adds a `Context` field - no type change, no rewiring.

### Lifecycle Hooks

Actors can implement optional lifecycle methods:

- **`on_start`** - Called once when the actor starts
- **`on_shutdown`** - Called during graceful shutdown
- **`on_error`** - Handle errors (swallow or propagate)

```rust
async fn on_start(&mut self) -> Result<()> {
    println!("Actor starting...");
    Ok(())
}

fn on_error(&self, error: Error) -> Result<()> {
    eprintln!("Error: {}", error);
    Ok(())  // Swallow error, continue running
}
```

## Context

The `Context` provides actors with capabilities to interact with the system. **Context is optional** - actors that only consume events (without sending) don't need to store it:

```rust
// Pure consumer - no context needed
struct Logger;

impl Actor for Logger {
    type Event = MyEvent;

    async fn handle_event(&mut self, envelope: &Envelope<Self::Event>) -> Result<()> {
        println!("Received: {:?}", envelope.event());
        Ok(())
    }
}

// Producer/processor - stores context to send events
struct Processor {
    ctx: Context<MyEvent>,
}
```

Context capabilities:

```rust
// Send events
ctx.send(NetworkEvent::PacketReceived(data)).await?;

// Send with correlation (for tracking related events)
ctx.send_child_event(ResponseEvent::Ok, envelope.meta()).await?;

// Stop this actor
ctx.stop();

// Get actor's name
let name = ctx.actor_name();
```

## Supervisor

The `Supervisor` manages actor lifecycles and provides registration APIs.

### Simple API

```rust
let mut sup = Supervisor::<NetworkEvent, NetworkTopic>::default();

sup.add_actor("ingress", |ctx| IngressActor::new(ctx), &[NetworkTopic::Ingress])?;
sup.add_actor("egress", |ctx| EgressActor::new(ctx), &[NetworkTopic::Egress])?;
sup.add_actor("monitor", MonitorActor::new, Subscribe::all())?; // no closure needed
```

### Actor Builder

Use `build_actor` when an actor needs non-default configuration (e.g. a larger channel):

```rust
sup.build_actor("writer", |ctx| Writer::new(ctx))
    .topics(&[NetworkTopic::Ingress])
    .channel_capacity(512)
    .build()?;
```

See [Advanced Topics - Per-Actor Config](advanced.md#per-actor-config) for details.

### Runtime Control

```rust
// Start all actors (non-blocking)
sup.start().await?;

// Wait for completion
sup.join().await?;

// Or combine both
sup.run().await?;

// Graceful shutdown
sup.stop().await?;

// Send events from outside (Supervisor as actor)
sup.send(MyEvent::Data(42)).await?;
```

## StepAction

The `step()` method returns `StepAction` to control scheduling:

| Action | Behavior |
|--------|----------|
| `StepAction::Continue` | Run `step` again immediately |
| `StepAction::Yield` | Yield to runtime, then run again |
| `StepAction::AwaitEvent` | Pause until next event arrives |
| `StepAction::Backoff(Duration)` | Sleep, then run again |
| `StepAction::Never` | Disable `step` permanently (default) |

### Common Patterns

**Event producer with interval:**
```rust
async fn step(&mut self) -> Result<StepAction> {
    self.ctx.send(HeartbeatEvent).await?;
    Ok(StepAction::Backoff(Duration::from_secs(5)))
}
```

**External I/O source:**
```rust
async fn step(&mut self) -> Result<StepAction> {
    let data = self.websocket.read().await?;
    self.ctx.send(DataEvent(data)).await?;
    Ok(StepAction::Continue)
}
```

**Pure event processor (default):**
```rust
async fn step(&mut self) -> Result<StepAction> {
    Ok(StepAction::Never)
}
```

## ActorId

`ActorId` uniquely identifies a registered actor. It is returned by `Supervisor::add_actor()` and used throughout the system for:

- **Event metadata** - every envelope carries the sender's `ActorId`
- **Test assertions** - verify which actors sent/received events
- **Correlation** - track event flow between specific actors

```rust
// Returned when registering an actor
let producer: ActorId = sup.add_actor("producer", |ctx| Producer::new(ctx), &[DefaultTopic])?;

// Access sender from event metadata
async fn handle_event(&mut self, envelope: &Envelope<Self::Event>) -> Result<()> {
    let sender: &ActorId = envelope.meta().actor_id();
    println!("Event from: {}", sender.as_str());
    Ok(())
}

// Use in test harness
assert!(test.event(id).was_delivered_to(&consumer));
```

### Serialization

`ActorId` supports serialization (with the `serde` feature) and works correctly across IPC boundaries. Equality uses string comparison with a fast-path for pointer equality when IDs share the same memory allocation:

```rust
// Same-process: pointer comparison (O(1))
// Cross-process (after deserialization): string comparison
assert_eq!(local_id, deserialized_id);  // Works correctly in both cases
```

This makes `ActorId` suitable for distributed scenarios where actor references need to survive serialization.

## Envelope & Metadata

Events arrive wrapped in an `Envelope` containing metadata:

```rust
async fn handle_event(&mut self, envelope: &Envelope<Self::Event>) -> Result<()> {
    // Access the event
    let event = envelope.event();

    // Access metadata
    let sender = envelope.meta().actor_name();
    let event_id = envelope.meta().id();
    let correlation = envelope.meta().correlation_id();

    // Send correlated child event
    self.ctx.send_child_event(ResponseEvent::Ok, envelope.meta()).await?;

    Ok(())
}
```

Correlation tracking enables tracing event causality through the system.
