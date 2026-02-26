//! Flow control example: fast producer, slow consumer, expendable telemetry.
//!
//! Demonstrates how `OverflowPolicy` governs broker behavior when a subscriber
//! can't keep up. Three policies are used across three topics:
//!
//! - **Data** (`Block`): 1KB chunks that must all arrive. The broker waits for
//!   the slow consumer, naturally throttling the producer.
//! - **Command** (`Block`): Start/Done signals. Must be delivered — a dropped
//!   Done would hang the system forever.
//! - **Telemetry** (`Drop`): Byte counters. Expendable under load.
//!
//! The producer also uses `ctx.is_sender_full()` to skip telemetry sends when
//! the broker input channel (stage 1) is congested. This is cooperative — the
//! producer decides what to skip, rather than the broker deciding what to drop.
//!
//! Checksums verify data integrity: if Block works correctly, producer and
//! consumer checksums match despite the consumer being ~1000x slower.
//!
//! ```text
//! Producer ──Data──► Broker ──Block──► Consumer (1ms/event)
//!    │                  │
//!    └──Telemetry──► Broker ──Drop──► Telemetry actor
//! ```
//!
//! Try experimenting:
//! - Remove the sleep in Consumer to see full-speed throughput
//! - Change Data policy to Drop and watch checksums diverge
//! - Change Command policy to Drop and watch the system hang (Done is lost)

use std::{sync::Arc, time::Duration};

#[cfg(feature = "monitoring")]
use maiko::monitors::Tracer;
use maiko::{Actor, Context, OverflowPolicy, StepAction, Supervisor};

#[derive(maiko::Event, maiko::Label, Debug, Clone)]
enum Event {
    Start(usize),
    Data(Box<[u8; 1024]>),
    BytesSent(usize),
    Done,
}

#[derive(Clone, PartialEq, Eq, Hash, Debug, maiko::Label)]
enum Topic {
    Data,
    Command,
    Telemetry,
}

impl maiko::Topic<Event> for Topic {
    fn from_event(event: &Event) -> Self
    where
        Self: Sized,
    {
        match event {
            Event::Start(_) => Topic::Command,
            Event::Data(_) => Topic::Data,
            Event::Done => Topic::Command,
            Event::BytesSent(_) => Topic::Telemetry,
        }
    }

    fn overflow_policy(&self) -> OverflowPolicy {
        match self {
            Topic::Data => OverflowPolicy::Block, // all chunks must arrive
            Topic::Command => OverflowPolicy::Block, // Start/Done must arrive
            Topic::Telemetry => OverflowPolicy::Drop, // best-effort reporting
        }
    }
}

/// Fast producer: generates 1KB random chunks as fast as the system allows.
///
/// Waits for a Start(n) command, then emits n Data events followed by Done.
/// Periodically sends BytesSent telemetry — but only when the broker channel
/// isn't congested (`is_sender_full` check).
struct Producer {
    ctx: Context<Event>,
    cnt: usize,
    checksum: u64,
    bytes: usize,
}

impl Actor for Producer {
    type Event = Event;

    async fn handle_event(&mut self, envelope: &maiko::Envelope<Self::Event>) -> maiko::Result<()> {
        if let Event::Start(cnt) = envelope.event() {
            self.cnt = *cnt;
            self.checksum = 0;
            self.bytes = 0;
        }
        Ok(())
    }

    async fn step(&mut self) -> maiko::Result<StepAction> {
        if self.cnt == 0 {
            return Ok(StepAction::AwaitEvent);
        }

        let mut buf: [u8; 1024] = [0; 1024];
        getrandom::fill(&mut buf).map_err(|e| maiko::Error::External(Arc::new(e)))?;
        let data = Box::new(buf);

        self.checksum = self
            .checksum
            .wrapping_add(data.iter().map(|b| *b as u64).sum::<u64>());

        // This send will block when the consumer's channel is full (Block policy).
        // The backpressure naturally throttles the producer to the consumer's speed.
        self.ctx.send(Event::Data(data)).await?;

        self.cnt -= 1;
        self.bytes += buf.len();
        if self.cnt == 0 {
            println!("Producer checksum: {}", self.checksum);
            self.ctx.send(Event::Done).await?;
        } else if self.cnt % 50 == 0 && !self.ctx.is_sender_full() {
            // Skip telemetry when the broker channel is congested.
            // This avoids competing with Data events for stage 1 capacity.
            self.ctx.send(Event::BytesSent(self.bytes)).await?;
        }
        Ok(StepAction::Continue)
    }

    fn on_error(&self, error: maiko::Error) -> maiko::Result<()> {
        eprintln!("Producer error: {}", error);
        Ok(())
    }
}

/// Slow consumer: processes each 1KB chunk with a 1ms delay.
///
/// With 128-slot channels and 1ms/event processing, the consumer falls
/// behind almost immediately. Block policy on Data ensures no chunks are
/// lost — the producer simply waits.
struct Consumer {
    ctx: Context<Event>,
    checksum: u64,
}

impl Actor for Consumer {
    type Event = Event;
    async fn handle_event(&mut self, envelope: &maiko::Envelope<Self::Event>) -> maiko::Result<()> {
        match envelope.event() {
            Event::Done => {
                println!("Consumer checksum: {}", self.checksum);
                self.ctx.stop_runtime()?;
            }
            Event::Data(data) => {
                self.checksum = self
                    .checksum
                    .wrapping_add(data.iter().map(|b| *b as u64).sum::<u64>());
                // Simulating slow consumer — ~1ms per event
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
            _ => (),
        }
        Ok(())
    }

    fn on_error(&self, error: maiko::Error) -> maiko::Result<()> {
        eprintln!("Consumer error: {}", error);
        Ok(())
    }
}

/// Telemetry observer: prints byte transfer progress.
///
/// Subscribed to the Telemetry topic with Drop policy — some updates
/// may be discarded under load, which is fine for progress reporting.
struct Telemetry;
impl Actor for Telemetry {
    type Event = Event;
    async fn handle_event(&mut self, envelope: &maiko::Envelope<Self::Event>) -> maiko::Result {
        if let Event::BytesSent(bytes) = envelope.event() {
            println!("Transferred {bytes} bytes so far");
        }
        Ok(())
    }
}

async fn run() -> maiko::Result {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let mut sup = Supervisor::<Event, Topic>::default();
    sup.add_actor(
        "producer",
        |ctx| Producer {
            ctx,
            cnt: 0,
            checksum: 0,
            bytes: 0,
        },
        &[Topic::Command],
    )?;
    // Consumer uses build_actor for a larger mailbox — it processes events
    // slowly (1ms each), so a bigger buffer absorbs short bursts before
    // the Block policy kicks in and throttles the producer.
    sup.build_actor("consumer", |ctx| Consumer { ctx, checksum: 0 })
        .topics(&[Topic::Data, Topic::Command])
        .channel_capacity(256)
        .with_config(|c| c.with_max_events_per_tick(64))
        .build()?;
    sup.add_actor("telemetry", |_| Telemetry, [Topic::Telemetry])?;

    #[cfg(feature = "monitoring")]
    sup.monitors().add(Tracer).await;

    sup.start().await?;
    let start = std::time::Instant::now();
    sup.send(Event::Start(1000)).await?;
    sup.join().await?;
    println!("Elapsed: {:?}", start.elapsed());
    Ok(())
}

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("Error while executing example: {e}");
    }
}
