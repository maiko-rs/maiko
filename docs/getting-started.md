# Getting Started

This guide walks you through building a weather station system step by step, introducing Maiko concepts progressively. By the end, you'll have a multi-actor system with custom topics, periodic event production, monitoring, and tests.

## Prerequisites

- Rust (stable)
- A Tokio-based project

Add Maiko to your `Cargo.toml`:

```sh
cargo add maiko
```

## Step 1: Your First Actor

Let's start with a single actor that logs weather events.

```rust
use maiko::*;

// Events are messages that flow through the system.
// Derive Event (plus Clone and Debug) to make them routable.
#[derive(Event, Clone, Debug)]
enum WeatherEvent {
    Temperature(f64),
}

// An actor that logs weather events.
struct WeatherLogger;

impl Actor for WeatherLogger {
    type Event = WeatherEvent;

    async fn handle_event(&mut self, envelope: &Envelope<Self::Event>) -> Result<()> {
        println!("[weather] {:?}", envelope.event());
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Create a supervisor. DefaultTopic routes all events to all subscribers.
    let mut sup = Supervisor::<WeatherEvent>::default();

    // Register the actor. It subscribes to DefaultTopic (receives everything).
    sup.add_actor("logger", |_ctx| WeatherLogger, &[DefaultTopic])?;

    sup.start().await?;

    // Send an event from outside the system.
    sup.send(WeatherEvent::Temperature(22.5)).await?;

    sup.stop().await
}
```

Key points:
- **Events** implement `Event` (via derive), `Clone`, and `Debug`
- **Actors** implement `Actor` with a `handle_event` method
- **Supervisor** manages actor lifecycles - you register actors, start the system, and stop it
- **DefaultTopic** is the simplest routing: all events go to all subscribed actors
- The `|_ctx| WeatherLogger` factory receives a `Context` - we ignore it here since this actor doesn't send events

## Step 2: Adding a Second Actor

Add an alerter that reacts to high temperatures by emitting an alert event.

```rust
use maiko::*;

#[derive(Event, Clone, Debug)]
enum WeatherEvent {
    Temperature(f64),
    Alert(String),
}

// Alerter needs Context to send events.
struct Alerter {
    ctx: Context<WeatherEvent>,
    threshold: f64,
}

impl Actor for Alerter {
    type Event = WeatherEvent;

    async fn handle_event(&mut self, envelope: &Envelope<Self::Event>) -> Result<()> {
        if let WeatherEvent::Temperature(temp) = envelope.event() {
            if *temp > self.threshold {
                self.ctx.send(WeatherEvent::Alert(
                    format!("High temperature: {:.1}°C", temp),
                )).await?;
            }
        }
        Ok(())
    }
}

struct WeatherLogger;

impl Actor for WeatherLogger {
    type Event = WeatherEvent;

    async fn handle_event(&mut self, envelope: &Envelope<Self::Event>) -> Result<()> {
        println!("[weather] {:?}", envelope.event());
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut sup = Supervisor::<WeatherEvent>::default();

    sup.add_actor("alerter", |ctx| Alerter { ctx, threshold: 35.0 }, &[DefaultTopic])?;
    sup.add_actor("logger", |_ctx| WeatherLogger, &[DefaultTopic])?;

    sup.start().await?;

    sup.send(WeatherEvent::Temperature(22.5)).await?;
    sup.send(WeatherEvent::Temperature(38.0)).await?;

    // Give actors time to process.
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    sup.stop().await
}
```

This works: the alerter processes temperatures and emits alerts, the logger observes everything. But with `DefaultTopic`, every subscribed actor receives every event. The alerter filters by pattern match in `handle_event`, silently ignoring `Alert` events it doesn't care about. As systems grow - more event types, more actors - you want routing to be explicit. Custom topics solve this.

## Step 3: Custom Topics

Topics control which actors receive which events. Define a topic enum and implement `Topic<E>` to map events to topics:

```rust
use maiko::*;

#[derive(Event, Clone, Debug)]
enum WeatherEvent {
    Temperature(f64),
    Humidity(f64),
    Alert(String),
}

// Topics determine routing. Each event maps to exactly one topic.
#[derive(Debug, Hash, Eq, PartialEq, Clone)]
enum WeatherTopic {
    Reading,
    Alert,
}

impl Topic<WeatherEvent> for WeatherTopic {
    fn from_event(event: &WeatherEvent) -> Self {
        match event {
            WeatherEvent::Temperature(_) => WeatherTopic::Reading,
            WeatherEvent::Humidity(_) => WeatherTopic::Reading,
            WeatherEvent::Alert(_) => WeatherTopic::Alert,
        }
    }
}

struct Alerter {
    ctx: Context<WeatherEvent>,
    threshold: f64,
}

impl Actor for Alerter {
    type Event = WeatherEvent;

    async fn handle_event(&mut self, envelope: &Envelope<Self::Event>) -> Result<()> {
        if let WeatherEvent::Temperature(temp) = envelope.event() {
            if *temp > self.threshold {
                self.ctx.send(WeatherEvent::Alert(
                    format!("High temperature: {:.1}°C", temp),
                )).await?;
            }
        }
        Ok(())
    }
}

struct WeatherLogger;

impl Actor for WeatherLogger {
    type Event = WeatherEvent;

    async fn handle_event(&mut self, envelope: &Envelope<Self::Event>) -> Result<()> {
        println!("[weather] {:?}", envelope.event());
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Specify both the event type AND the topic type.
    let mut sup = Supervisor::<WeatherEvent, WeatherTopic>::default();

    // Alerter subscribes only to Reading - receives temperatures, not its own alerts.
    sup.add_actor("alerter", |ctx| Alerter { ctx, threshold: 35.0 }, &[WeatherTopic::Reading])?;

    // Logger subscribes to everything.
    sup.add_actor("logger", |_ctx| WeatherLogger, Subscribe::all())?;

    sup.start().await?;

    sup.send(WeatherEvent::Temperature(22.5)).await?;
    sup.send(WeatherEvent::Temperature(38.0)).await?;

    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    sup.stop().await
}
```

Now the alerter only receives `Reading` events. Its `Alert` output goes to the logger (subscribed to all) but the alerter never sees it. Topics give you precise control over event routing without actors knowing about each other.

Want SMS alerts alongside email? Add an actor subscribed to `WeatherTopic::Alert`. The existing code doesn't change.

## Step 4: Producing Events with `step()`

So far, events come from `sup.send()` outside the system. Real sensors produce events on their own. The `step()` method lets actors do periodic work:

```rust
use std::time::Duration;
use maiko::*;

# #[derive(Event, Clone, Debug)]
# enum WeatherEvent {
#     Temperature(f64),
#     Alert(String),
# }
#
# #[derive(Debug, Hash, Eq, PartialEq, Clone)]
# enum WeatherTopic {
#     Reading,
#     Alert,
# }
#
# impl Topic<WeatherEvent> for WeatherTopic {
#     fn from_event(event: &WeatherEvent) -> Self {
#         match event {
#             WeatherEvent::Temperature(_) => WeatherTopic::Reading,
#             WeatherEvent::Alert(_) => WeatherTopic::Alert,
#         }
#     }
# }
struct TempSensor {
    ctx: Context<WeatherEvent>,
}

impl Actor for TempSensor {
    type Event = WeatherEvent;

    async fn handle_event(&mut self, _envelope: &Envelope<Self::Event>) -> Result<()> {
        Ok(())
    }

    // step() runs periodically. StepAction controls the schedule.
    async fn step(&mut self) -> Result<StepAction> {
        // Simulate a reading (in reality, read from hardware or an API).
        let temp = 20.0 + (rand::random::<f64>() * 20.0);
        self.ctx.send(WeatherEvent::Temperature(temp)).await?;

        // Wait 5 seconds before the next reading.
        Ok(StepAction::Backoff(Duration::from_secs(5)))
    }
}
```

Register it with no subscriptions - it produces events but doesn't consume any:

```rust
sup.add_actor("sensor", |ctx| TempSensor { ctx }, Subscribe::none())?;
```

`StepAction` controls what happens after each step:

| Action | Behavior |
|--------|----------|
| `Continue` | Run step again immediately |
| `Yield` | Yield to runtime, then run again |
| `AwaitEvent` | Pause until next event arrives |
| `Backoff(Duration)` | Sleep, then run again |
| `Never` | Disable step permanently (default) |

Now the sensor produces readings every 5 seconds. No external trigger needed.

## Step 5: Monitoring

Maiko provides a monitoring API for observing event flow. Enable it with the `monitoring` feature:

```toml
[dependencies]
maiko = { version = "0.2", features = ["monitoring"] }
```

The built-in `Tracer` logs event lifecycle to the `tracing` crate. Add `tracing-subscriber` for console output:

```sh
cargo add tracing-subscriber
```

```rust
use maiko::monitors::Tracer;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing - without this, Tracer output goes nowhere.
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .init();

    let mut sup = Supervisor::<WeatherEvent, WeatherTopic>::default();

    // Add tracer before starting
    sup.monitors().add(Tracer).await;

    sup.add_actor("sensor", |ctx| TempSensor { ctx }, Subscribe::none())?;
    sup.add_actor("alerter", |ctx| Alerter { ctx, threshold: 35.0 }, &[WeatherTopic::Reading])?;
    sup.add_actor("logger", |_ctx| WeatherLogger, Subscribe::all())?;

    sup.start().await?;

    sup.send(WeatherEvent::Temperature(38.0)).await?;
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    sup.stop().await
}
```

With a `tracing` subscriber configured, you'll see event flow through the system:

```
TRACE dispatched: Temperature(38.0) -> alerter (topic: Reading)
TRACE dispatched: Temperature(38.0) -> logger (topic: Reading)
DEBUG handled: Temperature(38.0) by alerter
TRACE dispatched: Alert("High temperature: 38.0°C") -> logger (topic: Alert)
DEBUG handled: Alert("High temperature: 38.0°C") by logger
```

You can also write custom monitors by implementing the `Monitor` trait. See [Monitoring Documentation](monitoring.md) for the full API.

## Step 6: Testing

Maiko includes a test harness for asserting on event flow. Enable it in your `Cargo.toml`:

```toml
[dev-dependencies]
maiko = { version = "0.2", features = ["test-harness"] }
```

Write a test that verifies the alerter triggers on high temperatures:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_high_temperature_triggers_alert() -> Result<()> {
        let mut sup = Supervisor::<WeatherEvent, WeatherTopic>::default();

        let sensor = sup.add_actor("sensor", |ctx| TempSensor { ctx }, Subscribe::none())?;
        let alerter = sup.add_actor(
            "alerter",
            |ctx| Alerter { ctx, threshold: 35.0 },
            &[WeatherTopic::Reading],
        )?;
        let logger = sup.add_actor("logger", |_ctx| WeatherLogger, Subscribe::all())?;

        // Initialize harness BEFORE starting the supervisor.
        let mut test = Harness::new(&mut sup).await;
        sup.start().await?;

        // Start recording, inject an event, wait for propagation.
        test.record().await;
        let id = test.send_as(&sensor, WeatherEvent::Temperature(40.0)).await?;
        test.settle().await;

        // The temperature event was delivered to the alerter.
        assert!(test.event(id).was_delivered_to(&alerter));

        // The alerter produced an Alert event.
        let alerts = test.events().with_topic(&WeatherTopic::Alert);
        assert_eq!(1, alerts.count());

        // The logger received both the temperature and the alert.
        assert_eq!(2, test.actor(&logger).events_received());

        sup.stop().await
    }

    #[tokio::test]
    async fn test_normal_temperature_no_alert() -> Result<()> {
        let mut sup = Supervisor::<WeatherEvent, WeatherTopic>::default();

        let sensor = sup.add_actor("sensor", |ctx| TempSensor { ctx }, Subscribe::none())?;
        sup.add_actor(
            "alerter",
            |ctx| Alerter { ctx, threshold: 35.0 },
            &[WeatherTopic::Reading],
        )?;

        let mut test = Harness::new(&mut sup).await;
        sup.start().await?;

        test.record().await;
        test.send_as(&sensor, WeatherEvent::Temperature(22.0)).await?;
        test.settle().await;

        // No alert should be generated for normal temperatures.
        let alerts = test.events().with_topic(&WeatherTopic::Alert);
        assert_eq!(0, alerts.count());

        sup.stop().await
    }
}
```

The test harness lets you:
- **Inject events** as specific actors with `send_as`
- **Record** all event deliveries during a test window
- **Settle** - wait until the system is quiet
- **Assert** on delivery, topic routing, and event counts

For the full harness API (spies, event chains, queries), see [Test Harness Documentation](testing.md).

## Next Steps

- **[Core Concepts](concepts.md)** - detailed reference for events, topics, actors, context, and supervisor
- **[Monitoring](monitoring.md)** - custom monitors, metrics collection, event lifecycle hooks
- **[Testing](testing.md)** - full test harness API: spies, queries, event chains, matchers
- **[Advanced Topics](advanced.md)** - error handling, configuration, flow control, performance
- **[Why Maiko?](why-maiko.md)** - motivation, comparisons, and design philosophy
- **[Examples](../maiko/examples/)** - `pingpong.rs`, `guesser.rs`, `monitoring.rs`
