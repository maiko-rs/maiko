# Advanced Topics

This document covers advanced Maiko features and patterns beyond the basics: error handling strategies, runtime configuration, and performance considerations.

For Maiko's design philosophy - loose coupling through topics, unidirectional flow, actors as domain entities, and guidance on when Maiko fits vs. alternatives - see **[Why Maiko?](why-maiko.md#design-philosophy)**.


## Error Handling

Control how errors propagate through actors:

```rust
impl Actor for MyActor {
    type Event = MyEvent;

    async fn handle_event(&mut self, envelope: &Envelope<Self::Event>) -> Result<()> {
        // ... processing that might fail
        Ok(())
    }

    fn on_error(&self, error: Error) -> Result<()> {
        match &error {
            Error::MailboxClosed => {
                // Mailbox closed, probably shutting down
                eprintln!("Warning: {}", error);
                Ok(())  // Swallow, continue running
            }
            _ => {
                eprintln!("Fatal: {}", error);
                Err(error)  // Propagate, stop actor
            }
        }
    }
}
```

Returning `Ok(())` from `on_error` swallows the error and continues. Returning `Err(error)` stops the actor.

## Configuration

### Global Config

Fine-tune runtime behavior with `Config`:

```rust
let config = Config::default()
    .with_broker_channel_capacity(512)          // Broker input buffer (default: 256)
    .with_default_actor_channel_capacity(256)   // Actor mailbox default (default: 128)
    .with_default_max_events_per_tick(50);       // Events processed per tick (default: 10)

let mut sup = Supervisor::new(config);
```

| Option | Default | Description |
|--------|---------|-------------|
| `broker_channel_capacity` | 256 | Per-actor channel to broker (stage 1). Producer blocks when its channel is full. |
| `default_actor_channel_capacity` | 128 | Default mailbox size for new actors (stage 2). |
| `default_max_events_per_tick` | 10 | Max events an actor processes before yielding. Per-actor override via `ActorBuilder`. |
| `monitoring_channel_capacity` | 1024 | Buffer size used by "monitoring" feature |

### Per-Actor Config

Use `build_actor` to override settings for individual actors:

```rust
// Slow consumer with a larger mailbox to absorb bursts
sup.build_actor("writer", |ctx| Writer::new(ctx))
    .topics(&[Topic::Data])
    .channel_capacity(512)
    .build()?;

// Fast actor using global defaults
sup.add_actor("processor", |ctx| Processor::new(ctx), &[Topic::Data])?;
```

`add_actor` always uses global defaults. When an actor needs different settings
- typically slow consumers or actors handling bursty traffic - use
`build_actor`. For settings without a direct shorthand, use `with_config`:

```rust
sup.build_actor("consumer", |ctx| Consumer::new(ctx))
    .topics(&[Topic::Data])
    .channel_capacity(256)
    .with_config(|c| c.with_max_events_per_tick(64))
    .build()?;
```

## Parent Tracking

Track event causality with parent IDs:

```rust
async fn handle_event(&mut self, envelope: &Envelope<Self::Event>) -> Result<()> {
    // Check if this event has a parent
    if let Some(parent_id) = envelope.meta().parent_id() {
        println!("This event was triggered by: {}", parent_id);
    }

    // Send a child event linked to this one
    self.ctx.send_child_event(
        MyEvent::Response(data),
        envelope.id()  // Links new event to this one
    ).await?;

    Ok(())
}
```

Child events carry their parent's ID as `parent_id`, enabling tracing of event chains through the system. The [test harness](testing.md) uses parent IDs to track event propagation - `EventChain`, `ActorTrace`, and `EventTrace` let you assert on causal chains without manual bookkeeping.

## Flow Control

Events pass through two channel stages:

1. **Stage 1** (producer → broker): per-actor bounded channel. Always blocks when full.
2. **Stage 2** (broker → subscriber): per-actor bounded channel. Behavior depends on the topic's `OverflowPolicy`.

### Overflow Policies

Override `Topic::overflow_policy()` to control stage 2 behavior:

| Policy | Behavior | Use case |
|--------|----------|----------|
| `Fail` (default) | Close subscriber's channel, actor terminates | Surfaces problems immediately |
| `Drop` | Discard event, continue | Telemetry, metrics, expendable data |
| `Block` | Wait for space (broker pauses) | Commands, data that must arrive |

```rust
fn overflow_policy(&self) -> OverflowPolicy {
    match self {
        MyTopic::Control   => OverflowPolicy::Block,
        MyTopic::Data      => OverflowPolicy::Block,
        MyTopic::Telemetry => OverflowPolicy::Drop,
    }
}
```

### Producer-Side Control

Producers can check their own channel congestion with `Context::is_sender_full()` to skip non-essential events:

```rust
async fn step(&mut self) -> Result<StepAction> {
    // Only send telemetry when the system isn't busy
    if !self.ctx.is_sender_full() {
        self.ctx.send(Event::Metrics(self.stats())).await?;
    }
    Ok(StepAction::Backoff(Duration::from_secs(1)))
}
```

### Single-Broker Limitation

With a single broker (current architecture), `Block` on one topic delays dispatch to all other topics while the broker waits. For most systems this delay is acceptable. Multi-broker support (planned) will eliminate cross-topic interference.

## Performance Considerations

### Channel Sizing

- **Too small**: Frequent backpressure, `Block` policies stall the broker often
- **Too large**: Memory overhead, delayed backpressure signals

Start with defaults and tune based on profiling.

### Event Processing Rate

`max_events_per_tick` balances:
- **Higher values**: Better throughput, longer latency for other actors
- **Lower values**: Fairer scheduling, more context switching

### Payload Size

Maiko can handle larger events efficiently as each `Envelope` is wrapped in `Arc` before sending.
It means there is no memory overhead while sending the same event to multiple actors.
