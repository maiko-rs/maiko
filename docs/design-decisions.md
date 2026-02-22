# Design Decisions

Architectural choices that shaped Maiko, and why.

## EventId: UUID v4

`EventId` is a `u128` generated via UUID v4. We evaluated several alternatives:

- **Monotonic u64 counter** - fast and simple, but breaks cross-process uniqueness. Maiko events can traverse process boundaries (e.g., Charon's daemon communicates with its client over a Unix socket). Two processes with independent counters would produce colliding IDs.
- **Composite u128** (random process prefix + monotonic counter) - solves cross-process uniqueness, but reinvents UUID with more code and a custom scheme to document.
- **Composite u64** (smaller prefix + counter) - trades collision probability math and counter overflow limits for 8 bytes of savings per event. Not worth the constraints.

UUID v4 is globally unique, universally understood, zero-configuration, and the `uuid` crate is small. Monotonic ordering is not needed — `Meta.timestamp()` provides creation-order sorting.

## Correlation as a First-Class Concept

Every event carries an optional `correlation_id: Option<EventId>` in its metadata. This is a deliberate design choice - correlation lives in Maiko's infrastructure, not in user event payloads.

This enables the [test harness](docs/testing.md) to trace event propagation chains, generate Mermaid sequence diagrams, and verify actor flow - without requiring users to thread tracking IDs through their event types. The extra metadata field is a small cost for system-wide observability.

For cross-process scenarios, a bridge actor creates local events correlated to foreign IDs. Since both sides use the same `EventId` type (`u128`), foreign Maiko IDs fit natively as correlation targets. Mapping between Maiko IDs and non-Maiko systems is the integrator's responsibility.

## Backpressure Over Fire-and-Forget

`ctx.send().await` is the only way to publish events. There is no `try_send`. When the broker's channel is full, producers wait.

This is an intentional tradeoff. Pure actor independence suggests fire-and-forget, but that needs somewhere for events to go: unbounded queues risk memory exhaustion, bounded queues with silent drops cause data loss. Bounded queues with backpressure give a stable, predictable system.

That said, actors can participate in flow control cooperatively: `ctx.is_sender_full()` lets a producer check its own channel capacity and skip non-essential sends (e.g., telemetry) when congested. The system enforces backpressure; the actor chooses what to shed.

See the [FAQ](faq.md#why-is-there-no-try_send-in-context) for the full rationale.
