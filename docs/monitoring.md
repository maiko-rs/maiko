# Monitoring

Maiko provides a monitoring API for observing event flow through the system. Enable it with the `monitoring` feature:

```toml
[dependencies]
maiko = { version = "0.3", features = ["monitoring"] }
```

## Overview

The monitoring system provides hooks into the event lifecycle:
- **Event dispatched** - when the broker routes an event to a subscriber
- **Event delivered** - when an actor receives an event from its mailbox
- **Event handled** - when an actor finishes processing an event
- **Overflow** - when a subscriber's channel is full and an overflow policy is triggered
- **Errors** - when an actor's event handler returns an error
- **Actor lifecycle** - when actors register and stop

Monitors are useful for:
- **Debugging** - trace event flow through the system
- **Metrics** - collect timing, counts, and throughput data
- **Logging** - structured event logs for observability
- **Testing** - the [test harness](testing.md) is built on top of monitoring

## Basic Usage

Implement the `Monitor` trait and register it with the supervisor:

```rust
use maiko::{ActorId, Envelope, Event, Topic};
use maiko::monitoring::Monitor;

struct EventLogger;

impl<E: Event, T: Topic<E>> Monitor<E, T> for EventLogger {
    fn on_event_dispatched(&self, envelope: &Envelope<E>, topic: &T, receiver: &ActorId) {
        println!("[dispatched] {} -> {} (topic: {:?})",
            envelope.meta().actor_name(), receiver.as_str(), topic);
    }

    fn on_event_handled(&self, envelope: &Envelope<E>, topic: &T, receiver: &ActorId) {
        println!("[handled] {} processed by {}",
            envelope.id(), receiver.as_str());
    }
}

#[tokio::main]
async fn main() -> maiko::Result {
    let mut sup = maiko::Supervisor::<MyEvent>::default();

    // Register the monitor
    let handle = sup.monitors().add(EventLogger).await;

    // ... add actors and start ...

    sup.start().await?;
    // Events will now be logged

    sup.stop().await
}
```

## Built-in Monitors

Maiko provides ready-to-use monitors in the `maiko::monitors` module:

### Tracer

Logs event lifecycle to the `tracing` crate. Zero configuration needed:

```rust
use maiko::monitors::Tracer;

sup.monitors().add(Tracer).await;
```

### ActorMonitor

Tracks actor lifecycle and overflow counts. Clone it to query from any thread:

```rust
use maiko::monitors::ActorMonitor;

let monitor = ActorMonitor::new();
let query = monitor.clone();
sup.monitors().add(monitor).await;

// Later, from any thread:
query.is_alive(&actor_id);
query.overflow_count(&actor_id);
query.actors();          // snapshot of active actor IDs
query.stopped_actors();  // snapshot of stopped actor IDs
```

### Recorder

Records events to a JSON Lines file for replay or debugging. Requires `recorder` feature:

```toml
maiko = { version = "0.3", features = ["recorder"] }
```

```rust
use maiko::monitors::Recorder;

let recorder = Recorder::new("events.jsonl")?;
sup.monitors().add(recorder).await;
```

### Test Harness

The [test harness](testing.md) is a specialized monitor for testing. It captures events for inspection and assertion. Requires `test-harness` feature.

## Monitor Trait

The `Monitor` trait provides callbacks for different lifecycle events. All methods have default no-op implementations, so you only need to implement the ones you care about:

```rust
pub trait Monitor<E: Event, T: Topic<E>>: Send {
    /// Called when the broker dispatches an event to a subscriber.
    fn on_event_dispatched(&self, envelope: &Envelope<E>, topic: &T, receiver: &ActorId) {}

    /// Called when an actor receives an event from its mailbox.
    fn on_event_delivered(&self, envelope: &Envelope<E>, topic: &T, receiver: &ActorId) {}

    /// Called after an actor finishes processing an event.
    fn on_event_handled(&self, envelope: &Envelope<E>, topic: &T, receiver: &ActorId) {}

    /// Called when a new actor is registered in the system.
    fn on_actor_registered(&self, actor_id: &ActorId) {}

    /// Called when an actor's handler returns an error.
    fn on_error(&self, err: &str, actor_id: &ActorId) {}

    /// Called when an actor enters its step() method.
    fn on_step_enter(&self, actor_id: &ActorId) {}

    /// Called when an actor exits its step() method.
    fn on_step_exit(&self, step_action: &StepAction, actor_id: &ActorId) {}

    /// Called when a subscriber's channel is full (see OverflowPolicy).
    fn on_overflow(&self, envelope: &Envelope<E>, topic: &T, receiver: &ActorId, policy: OverflowPolicy) {}

    /// Called when an actor stops.
    fn on_actor_stop(&self, actor_id: &ActorId) {}
}
```

### Event Lifecycle

Events flow through these stages:

1. **Dispatched** - Broker determines the topic and sends to each subscriber
2. **Delivered** - Actor receives the event from its channel
3. **Handled** - Actor's `handle_event()` completes

If a subscriber's channel is full at step 1, `on_overflow` fires instead of `on_event_dispatched`. The overflow policy then determines what happens next (drop, block, or close the channel).

For a single event delivered to multiple actors, you'll see:
- One `on_event_dispatched` call per receiver (or `on_overflow` if the channel is full)
- One `on_event_delivered` call per receiver
- One `on_event_handled` call per receiver

## MonitorHandle

When you register a monitor, you receive a `MonitorHandle` for controlling it:

```rust
let handle = sup.monitors().add(MyMonitor).await;

// Pause/resume this monitor
handle.pause().await;
handle.resume().await;

// Flush with a settle window (wait for queue to be empty and stay empty)
handle.flush(Duration::from_millis(1)).await;

// Remove the monitor entirely
handle.remove().await;
```

## MonitorRegistry

The supervisor provides access to the monitor registry:

```rust
let registry = sup.monitors();

// Add monitors
let handle = registry.add(MyMonitor).await;

// Pause/resume all monitors
registry.pause().await;
registry.resume().await;
```

## Example: Metrics Collector

```rust
use std::sync::atomic::{AtomicUsize, Ordering};

struct MetricsMonitor {
    dispatched: AtomicUsize,
    handled: AtomicUsize,
    errors: AtomicUsize,
}

impl MetricsMonitor {
    fn new() -> Self {
        Self {
            dispatched: AtomicUsize::new(0),
            handled: AtomicUsize::new(0),
            errors: AtomicUsize::new(0),
        }
    }

    fn report(&self) {
        println!("Dispatched: {}", self.dispatched.load(Ordering::Relaxed));
        println!("Handled: {}", self.handled.load(Ordering::Relaxed));
        println!("Errors: {}", self.errors.load(Ordering::Relaxed));
    }
}

impl<E: Event, T: Topic<E>> Monitor<E, T> for MetricsMonitor {
    fn on_event_dispatched(&self, _: &Envelope<E>, _: &T, _: &ActorId) {
        self.dispatched.fetch_add(1, Ordering::Relaxed);
    }

    fn on_event_handled(&self, _: &Envelope<E>, _: &T, _: &ActorId) {
        self.handled.fetch_add(1, Ordering::Relaxed);
    }

    fn on_error(&self, _: &str, _: &ActorId) {
        self.errors.fetch_add(1, Ordering::Relaxed);
    }
}
```

## Performance Considerations

### Fast-Path Optimization

When no monitors are active (none registered, or all paused), monitoring has **near-zero overhead**. The system uses an atomic boolean flag that's checked before any monitoring work is done:

```rust
// In the broker/actor controller - this is the only cost when monitoring is inactive
if self.monitoring.is_active() {
    // ... send monitoring event
}
```

### When Monitors Are Active

With active monitors:
- Each event dispatch/delivery/handle generates a monitoring event
- Events flow through an async channel to the monitoring dispatcher
- The dispatcher calls each monitor's callbacks synchronously

**Recommendations:**
- Keep monitor callbacks fast and non-blocking
- Use `Ordering::Relaxed` for atomic counters (sufficient for metrics)
- Consider batching if you need to send data to external systems
- Pause monitors when not needed

### Panic Safety

If a monitor panics during a callback, it is automatically removed from the registry. Other monitors continue operating normally. The panic is logged via `tracing::error!`.

### Channel Sizing

The monitoring channel has a default capacity of 1024 messages. If monitors can't keep up with event throughput, messages may be dropped (the system uses `try_send` to avoid blocking the broker). Configure via:

```rust
let config = SupervisorConfig::default().with_monitoring_channel_size(4096);
let sup = Supervisor::<MyEvent>::new(config);
```

## Relationship to Test Harness

The [test harness](testing.md) is built on top of the monitoring API. It registers a special `EventCollector` monitor that captures events for later inspection. This means:

- Test harness requires the `monitoring` feature (included in `test-harness`)
- You can use both custom monitors and the test harness together
- The same performance characteristics apply
