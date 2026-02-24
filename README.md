# Maiko

<div align="center">

**Structure your Tokio app as actors, not channels and spawns**

[![Crates.io](https://img.shields.io/crates/v/maiko.svg)](https://crates.io/crates/maiko)
[![Documentation](https://docs.rs/maiko/badge.svg)](https://docs.rs/maiko)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

</div>

---

## What is Maiko?

**Maiko** helps you build multi-task Tokio applications without manually wiring channels or spawning tasks. It's a topic-based pub/sub actor runtime: declare actors and subscriptions, and Maiko handles event routing and lifecycle management.

Actors are named components with their own state. They communicate through events routed by topic, without knowing who's listening. Think Kafka-style pub/sub, but embedded in your Tokio application.

Maiko comes with a built-in [test harness](docs/testing.md) for asserting on event flow, correlation tracking for tracing event propagation, and monitoring hooks for observability - all without external infrastructure.

### No more channel spaghetti

Building concurrent Tokio applications often means manually creating, cloning, and wiring channels between tasks:

```rust
// Without Maiko: manual channel wiring
let (tx1, rx1) = mpsc::channel(32);
let (tx2, rx2) = mpsc::channel(32);
let (tx3, rx3) = mpsc::channel(32);
// Clone tx1 for task B, clone tx2 for task C, wire rx1 to...
```

```rust
// With Maiko: declare subscriptions, routing happens automatically
sup.add_actor("sensor",    |ctx| Sensor::new(ctx),    Subscribe::none())?;      // produces events
sup.add_actor("processor", |ctx| Processor::new(ctx), &[Topic::SensorData])?;   // handles sensor data
sup.add_actor("logger",    |ctx| Logger::new(ctx),    Subscribe::all())?;       // observes everything
```

### How it compares

| | Maiko | Raw Tokio | Actix/Ractor | Kafka |
|---|-------|-----------|--------------|-------|
| Routing | Topic-based pub/sub | Manual channels | Direct addressing | Topic-based pub/sub |
| Coupling | Loose (actors don't know each other) | Explicit wiring | Tight (need actor addresses) | Loose |
| Communication | Events | Channels | Request-response | Events |
| Lifecycle | Managed (start, stop, hooks) | Manual spawns | Managed | Managed |
| Testing | Built-in harness, spies, event chains | Roll your own | None built-in | External tools |
| Correlation | First-class (correlation IDs, chain tracing) | Manual | Manual | Headers |
| Scope | In-process | In-process | In-process | Distributed |

Maiko sits between raw Tokio and full actor frameworks. Think of it as moving from breadboard to PCB - same components, reliable traces, testable as a whole.

### Where it fits

Event-centric systems: processing stock ticks, device signals, telemetry pipelines, handling system
events, data transformation. Maiko optimizes for developer ergonomics and testability, not raw throughput. Not ideal for request-response APIs or RPC patterns.

For detailed comparisons, use cases, and guidance on when Maiko fits, see **[Why Maiko?](docs/why-maiko.md)**.

---

## Quick Start

For a step-by-step walkthrough, see **[Getting Started](docs/getting-started.md)**.

```sh
cargo add maiko
```

```rust
use maiko::*;

#[derive(Event, Clone, Debug)]
enum MyEvent {
    Hello(String),
}

struct Greeter;

impl Actor for Greeter {
    type Event = MyEvent;

    async fn handle_event(&mut self, envelope: &Envelope<Self::Event>) -> Result<()> {
        if let MyEvent::Hello(name) = envelope.event() {
            println!("Hello, {}!", name);
        }
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut sup = Supervisor::<MyEvent>::default();
    sup.add_actor("greeter", |_ctx| Greeter, &[DefaultTopic])?;

    sup.start().await?;
    sup.send(MyEvent::Hello("World".into())).await?;
    sup.stop().await
}
```

### Examples

See the [`examples/`](maiko/examples/) directory:

- **[`pingpong.rs`](maiko/examples/pingpong.rs)** - Event exchange between actors
- **[`guesser.rs`](maiko/examples/guesser.rs)** - Multi-actor game with topics and timing
- **[`monitoring.rs`](maiko/examples/monitoring.rs)** - Observing event flow

```bash
cargo run --example pingpong
cargo run --example guesser
```

---

## Core Concepts

| Concept | Description |
|---------|-------------|
| **Event** | Messages that flow through the system (`#[derive(Event)]`) |
| **Topic** | Routes events to interested actors |
| **Actor** | Processes events via `handle_event()` and produces events via `step()` |
| **Context** | Provides actors with `send()`, `stop()`, and metadata access |
| **OverflowPolicy** | Controls behavior when a subscriber's channel is full (`Fail`, `Drop`, `Block`) |
| **Supervisor** | Manages actor lifecycles and the runtime |
| **Envelope** | Wraps events with metadata (sender, correlation ID) |

For detailed documentation, see **[Core Concepts](docs/concepts.md)**.

---

## Test Harness

Maiko includes a test harness (built on the monitoring API) for observing and asserting on event flow:

```rust
#[tokio::test]
async fn test_event_delivery() -> Result<()> {
    let mut sup = Supervisor::<MyEvent>::default();
    let producer = sup.add_actor("producer", |ctx| Producer::new(ctx), &[DefaultTopic])?;
    let consumer = sup.add_actor("consumer", |ctx| Consumer::new(ctx), &[DefaultTopic])?;

    let mut test = Harness::new(&mut sup).await;
    sup.start().await?;

    test.record().await;
    let id = test.send_as(&producer, MyEvent::Data(42)).await?;
    test.settle().await;

    assert!(test.event(id).was_delivered_to(&consumer));
    assert_eq!(1, test.actor(&consumer).events_received());

    sup.stop().await
}
```

Enable with `features = ["test-harness"]`. See **[Test Harness Documentation](docs/testing.md)** for details.

---

## Monitoring

The monitoring API provides hooks into the event lifecycle - useful for debugging, metrics, and logging:

```rust
use maiko::monitoring::Monitor;

struct EventLogger;

impl<E: Event, T: Topic<E>> Monitor<E, T> for EventLogger {
    fn on_event_handled(&self, envelope: &Envelope<E>, topic: &T, receiver: &ActorId) {
        println!("[handled] {} by {}", envelope.id(), receiver.as_str());
    }
}

let handle = supervisor.monitors().add(EventLogger).await;
```

Enable with `features = ["monitoring"]`. See **[Monitoring Documentation](docs/monitoring.md)** for details.

---

## Documentation

- **[Getting Started](docs/getting-started.md)** - Step-by-step tutorial
- **[Why Maiko?](docs/why-maiko.md)** - Motivation, comparisons, design philosophy
- **[Core Concepts](docs/concepts.md)** - Events, Topics, Actors, Context, Supervisor
- **[FAQ](docs/faq.md)** - Common questions answered
- **[Monitoring](docs/monitoring.md)** - Event lifecycle hooks, metrics collection
- **[Test Harness](docs/testing.md)** - Recording, spies, queries, assertions
- **[Advanced Topics](docs/advanced.md)** - Error handling, configuration, flow control
- **[API Reference on docs.rs](https://docs.rs/maiko)** - Complete API documentation

---

## Roadmap

**Near-term:**
- Dynamic actor spawning/removal at runtime
- Improved lifecycle strategies (restart, retry, etc)

**Future:**
- `maiko-actors` crate with reusable actors (IPC bridges, WebSocket, OpenTelemetry)
- Cross-process communication via bridge actors

---

## Used In

[Charon](https://github.com/ddrcode/charon) is a USB keyboard pass-through device built on Raspberry Pi. Its daemon uses Maiko actors to read input from multiple keyboards, map and translate key events, output USB HID to the host, and coordinate telemetry, IPC, and power management. Each concern is an actor; topics route events by type (key events, HID reports, commands, telemetry).

---

## Current State

Maiko is battle-tested in the [Charon](https://github.com/ddrcode/charon) project, where it runs continuously, but it's not yet production-grade. I'd describe it as solid for happy-path scenarios and still maturing for rainy days. Flow control with per-topic overflow policies is in place, supervision is minimal, and improved error handling and recovery strategies are planned.

For now, Maiko demonstrates what it wants to be. That's the state I wanted to reach before sharing it with a wider audience. Want to help shape what comes next? See below.

---

## Contributing

Contributions welcome! Whether it's a bug report, feature idea, or pull request - all input is appreciated.

- **Try it out** and [let us know what you think](https://github.com/maiko-rs/maiko/discussions/41)
- **Report issues** via [GitHub Issues](https://github.com/maiko-rs/maiko/issues)
- **Looking to contribute code?** Check out [good first issues](https://github.com/maiko-rs/maiko/issues?q=is%3Aissue+state%3Aopen+label%3A%22good+first+issue%22)

See **[CONTRIBUTING.md](https://github.com/maiko-rs/maiko/blob/main/CONTRIBUTING.md)** for branching guidelines, issue labels, and how to submit a PR.

---

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT)
at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this
crate by you, as defined in the Apache-2.0 license, shall be dual licensed as above,
without any additional terms or conditions.
