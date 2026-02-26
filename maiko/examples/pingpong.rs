//! Ping-Pong Example
//!
//! This example demonstrates event-driven communication between two actors using
//! **event-as-topic** routing via the `SelfRouting` derive.
//!
//! # Key Concepts Demonstrated
//!
//! ## 1. Topic-Based Routing
//!
//! Actors subscribe to specific event variants:
//! - "Ping" actor subscribes to `Pong` events only
//! - "Pong" actor subscribes to `Ping` events only
//!
//! ## 2. Event-as-Topic Pattern with `SelfRouting`
//!
//! The `#[derive(SelfRouting)]` macro makes the event type serve as both:
//! - The **event payload** (what gets sent)
//! - The **routing topic** (how it gets filtered)
//!
//! This is a common pattern in distributed systems (e.g., Kafka topics named after
//! event types). Each event variant becomes its own routing category.
//!
//! When an event is sent, the broker routes it only to actors subscribed to that
//! variant's topic. This creates a natural ping-pong without actors knowing about
//! each other.

use maiko::*;

/// Event types for the ping-pong system.
///
/// The `SelfRouting` derive implements `Topic<PingPongEvent> for PingPongEvent`,
/// enabling event-as-topic routing where each event variant becomes its own topic.
/// A `Ping` event routes to actors subscribed to the `Ping` topic, and `Pong` to `Pong`.
#[derive(Event, SelfRouting, Clone, Debug, Hash, PartialEq, Eq)]
enum PingPongEvent {
    Ping,
    Pong,
}

/// A simple actor that responds to ping-pong events.
struct PingPong {
    ctx: Context<PingPongEvent>,
}

impl Actor for PingPong {
    type Event = PingPongEvent;

    /// Handle incoming events by responding with the opposite event.
    async fn handle_event(&mut self, envelope: &Envelope<Self::Event>) -> Result {
        println!(
            "Event: {:?} received by {} actor",
            envelope.event(),
            self.ctx.actor_name()
        );
        let response = match envelope.event() {
            PingPongEvent::Ping => PingPongEvent::Pong,
            PingPongEvent::Pong => PingPongEvent::Ping,
        };

        // Send the response - broker will route it to the subscribed actor
        self.ctx.send(response).await
    }
}

/// An observer actor that counts all events without participating in the ping-pong.
///
/// This actor demonstrates:
/// - **Multi-topic subscription**: subscribes to both Ping and Pong events
/// - **Fan-out routing**: receives all events alongside the responders
/// - **Pure consumer pattern**: observes events without emitting new ones
struct Counter {
    count: u32,
}
impl Actor for Counter {
    type Event = PingPongEvent;

    async fn handle_event(&mut self, _envelope: &Envelope<Self::Event>) -> Result {
        self.count += 1;
        Ok(())
    }

    async fn on_shutdown(&mut self) -> Result {
        println!("Total events processed: {}", self.count);
        Ok(())
    }
}

#[tokio::main]
pub async fn main() -> Result {
    // Create a supervisor with PingPongEvent as both the event and topic type
    let mut sup = Supervisor::<PingPongEvent, PingPongEvent>::default();

    // Adds actors that subscribes ONLY to one type of event
    sup.add_actor("Ping", |ctx| PingPong { ctx }, [PingPongEvent::Pong])?;
    sup.add_actor("Pong", |ctx| PingPong { ctx }, [PingPongEvent::Ping])?;

    // Add "Counter" actor that subscribes to both events.
    sup.add_actor(
        "Counter",
        |_ctx| Counter { count: 0 },
        &[PingPongEvent::Ping, PingPongEvent::Pong],
    )?;

    // Start the supervisor (spawns the broker and actor tasks)
    sup.start().await?;

    // Kick off the ping-pong by sending the initial Ping event
    // Due to topic routing, only the "Pong" actor (subscribed to Ping) receives this
    sup.send(PingPongEvent::Ping).await?;

    // Let the ping-pong run for a brief period
    tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;

    // Gracefully stop the system
    sup.stop().await?;

    println!("Done");
    Ok(())
}
