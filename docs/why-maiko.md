# Why Maiko?

## The Problem

Building concurrent Tokio applications often means manually creating, cloning, and wiring channels between tasks:

```rust
let (tx1, rx1) = mpsc::channel(32);
let (tx2, rx2) = mpsc::channel(32);
let (tx3, rx3) = mpsc::channel(32);
// Clone tx1 for task B, clone tx2 for task C, wire rx1 to...
```

This scales poorly. At 5+ communicating tasks, channel wiring becomes the dominant source of complexity - hard to reason about, hard to test, hard to change.

Traditional actor frameworks (Actix, Ractor) solve the wiring problem but introduce tight coupling: every message requires knowing the recipient's address. Adding a new consumer means modifying every producer. The framework manages lifecycles, but your components are bound to each other.

## What Maiko Offers

Maiko replaces both problems with a single model: actors subscribe to topics, and a broker handles the routing.

No channels to wire - you declare what each actor cares about, and events arrive automatically. Adding a consumer never changes a producer. No addresses to pass around - actors don't know each other, only the event types they publish and subscribe to.

On top of that routing core:

- **Lifecycle management** - actors have `on_start`, `on_shutdown`, and `on_error` hooks. No manual `spawn` or `JoinHandle` tracking.
- **Built-in test harness** - record events, inject them as specific actors, assert on delivery. No async timing hacks.
- **Correlation tracking** - every event carries metadata linking it to its cause, enabling tracing of event chains through the system.
- **Monitoring hooks** - plug in observers for debugging, metrics, and logging without modifying actors.
- **Flow control** - per-topic overflow policies (`Fail`, `Drop`, `Block`) and per-actor channel sizing.

## When Maiko Fits

Maiko excels at **event-centric systems** where data flows through independent processors.

**Long-running daemons** — the comfort zone. Monitoring agents, IoT data collectors, system daemons. Actors map naturally to hardware or service boundaries. Topics route by severity, data type, or lifecycle stage.

**Data pipelines** — sensor readings, stock ticks, telemetry streams. Events flow through processors that normalize, enrich, filter, and store. Each stage is an actor; adding a stage doesn't change existing ones.

**Batch processors** — Maiko isn't only for daemons. Exchange tick data processing, file transformation pipelines, ETL jobs. "Runs, does a job, ends" is a valid pattern. Dynamic actors (planned) will make per-file or per-unit processing natural.

**Domains where this pattern fits:**

- **IoT / Embedded** — device signals, sensor readings, hardware events
- **Telemetry** — log processing, metrics aggregation, monitoring
- **Pipelines** — data transformation, enrichment, routing
- **Background jobs** — task coordination, workflow orchestration
- **Game engines** — entity systems, input handling
- **Reactive architectures** — event sourcing, CQRS

### Multi-role vs. multi-instance

Multi-task Tokio applications typically fall into two categories: many instances of the same task (e.g., one HTTP handler per request) or multiple tasks doing different things (e.g., a reader, a processor, and a writer cooperating).

Maiko's topic-based routing naturally shines for the multi-role case - each actor has a distinct responsibility, and topics wire them together. But it handles multi-instance just as well: multiple temperature sensors, multiple keyboard readers, multiple file processors all work fine. They're separate actors with separate state, publishing to the same topics. In [Charon](https://github.com/ddrcode/charon), multiple keyboard input actors coexist without knowing about each other.

Where Maiko is *not* the right fit is high-cardinality spawning - thousands of identical short-lived handlers (e.g., one per HTTP request). For that pattern, raw Tokio `spawn` is simpler and more efficient.

## When to Choose Alternatives

- **Request-response APIs** (REST, gRPC) - Actix Web or similar
- **RPC-style communication** - Ractor or Actix. Maiko is fire-and-forget; there's no "call this actor and get a response."
- **Distributed actors across machines** - Ractor. Maiko is in-process only.
- **Maximum performance with zero allocation** - Stakker. Maiko prioritizes ergonomics over bare-metal speed.
- **2-3 tasks with simple wiring** - raw Tokio is simpler. If the channel wiring fits in your head, you don't need a framework.
- **Complex supervision trees** - Ractor. Maiko has a single flat supervisor; restart strategies are planned but not yet available.

## How It Compares

| | Maiko | Raw Tokio | Actix | Ractor | Stakker |
|---|-------|-----------|-------|--------|---------|
| **Routing** | Topic-based pub/sub | Manual channels | Direct addressing | Direct addressing | Direct addressing |
| **Coupling** | Loose (actors don't know each other) | Explicit wiring | Tight (need actor addresses) | Tight (need actor addresses) | Tight |
| **Communication** | Events | Channels | Request-response | Request-response | Messages |
| **Discovery** | Subscribe to topics | Manual | Pass addresses manually | Registry lookup | Pass references |
| **Lifecycle** | Managed (start, stop, hooks) | Manual spawns | Managed | Managed | Managed |
| **Testing** | Built-in harness, spies, chains | Roll your own | None built-in | None built-in | None built-in |
| **Correlation** | First-class (IDs, chain tracing) | Manual | Manual | Manual | Manual |
| **Best for** | Event-driven pipelines | Full control | Web services, RPC | Distributed systems | Low-latency loops |

**When to choose Maiko over raw Tokio:**
- You have 5+ communicating tasks and channel wiring is getting messy
- You want built-in testing for event flow verification
- You want correlation tracking and observability without extra infrastructure
- You prefer declaring subscriptions over manually cloning senders

**When to choose Maiko over actor frameworks:**
- Your system is naturally event-driven (sensors, ticks, signals)
- You are looking for a lightweight solution
- You want loose coupling between components
- You don't need request-response patterns
- You prefer "publish and forget" over "send and await reply"

## Example: Weather Station

Imagine a weather monitoring system with multiple sensors:

```
┌─────────────┐  ┌─────────────┐  ┌─────────────┐
│ Temperature │  │  Humidity   │  │   Wind      │
│   Sensor    │  │   Sensor    │  │   Sensor    │
└──────┬──────┘  └──────┬──────┘  └──────┬──────┘
       │                │                │
       └────────────────┼────────────────┘
                        │
                        ▼ Topic: SensorReading
       ┌────────────────┼────────────────┐
       │                │                │
       ▼                ▼                ▼
┌─────────────┐  ┌─────────────┐  ┌─────────────┐
│ Normalizer  │  │  Database   │  │   Alert     │
│  (C° → F°)  │  │   Writer    │  │  Monitor    │
└──────┬──────┘  └─────────────┘  └──────┬──────┘
       │                                 │
       ▼ Topic: NormalizedReading        ▼ Topic: Alert
┌─────────────┐                   ┌─────────────┐
│  Hourly     │                   │   Email     │
│  Reporter   │                   │   Sender    │
└─────────────┘                   └─────────────┘
```

Each box is an actor. They don't know about each other - they publish events and subscribe to topics. Adding a new sensor? Register another actor. Want SMS alerts? Add an actor subscribed to `Alert`. The existing code doesn't change.

For a step-by-step implementation of this example, see [Getting Started](getting-started.md).

## Design Philosophy

### Loose Coupling Through Topics

Maiko actors don't know about each other. They only know the events they can send and the topics they subscribe to:

```rust
// Traditional actors (tight coupling)
actor_ref.tell(message);  // Must know the actor's address

// Maiko (loose coupling)
ctx.send(event).await?;   // Only knows about event types
```

This decoupling enables optimization. Because actors never hold direct references to each other, Maiko can rewire internals based on subscription topology — 1:1 mappings can bypass the broker, fan-out can be parallelized — all invisible to user code, testing, and observability.

### Unidirectional Flow

Events flow in one direction through the system:

```
Input → Parser → Validator → Processor → Output
```

No request-response, no callbacks, no circular dependencies. This makes systems easier to reason about, test, and debug.

### Actors as Domain Entities

Actors are named domain entities with responsibility boundaries, not anonymous dataflow nodes. The scanner isn't "a function from key events to HID reports" — it's *the scanner*. Identity enables the test harness API (`test.actor(&scanner)`), debugging, and domain reasoning.

### Convenient, Not Dumbed Down

The common case reads like intent, not like machinery. `handle_event` takes one argument (the thing that happened). Context through `self.ctx`. You don't need to understand the runtime to write your first actor.

Configuration follows the same principle: `Config::default()` is safe. Tuning is discoverable, not required. Sensible defaults for 80% of cases, clear escape hatches for the rest.
