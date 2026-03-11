//! Benchmarks end-to-end Actor -> Broker -> Actor transport throughput.
//!
//! Matrix:
//! - message count: 1_000, 10_000, 100_000
//! - payload size: 32B, 256B, 1KiB, 4KiB
//!
//! Throughput unit is messages/sec.
//!
//! Run with:
//! `cargo bench -p maiko --bench actor_to_actor_transport_maiko -- --noplot`
use std::{hint::black_box, time::Duration};

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use maiko::{
    Actor, Context, Envelope, Event, OverflowPolicy, Result, StepAction, Supervisor, Topic,
};
use tokio::{runtime::Builder, sync::oneshot, time::Instant};

#[derive(Debug, Clone)]
enum BenchEvent {
    Start,
    Payload(Vec<u8>),
}

impl Event for BenchEvent {}

#[derive(Debug, Hash, Eq, PartialEq, Clone, Copy)]
enum BenchTopic {
    Control,
    Payload,
}

impl Topic<BenchEvent> for BenchTopic {
    fn from_event(event: &BenchEvent) -> Self {
        match event {
            BenchEvent::Start => BenchTopic::Control,
            BenchEvent::Payload(_) => BenchTopic::Payload,
        }
    }

    fn overflow_policy(&self) -> OverflowPolicy {
        OverflowPolicy::Block
    }
}

/// Benchmark producer actor.
/// It sends `remaining` payload messages as fast as possible in `step`, after `BenchEvent::Start`.
struct Producer {
    ctx: Context<BenchEvent>,
    started: bool,
    remaining: usize,
    payload_template: Vec<u8>,
}

impl Actor for Producer {
    type Event = BenchEvent;

    async fn handle_event(&mut self, envelope: &Envelope<Self::Event>) -> Result {
        if let BenchEvent::Start = envelope.event() {
            self.started = true;
        }
        Ok(())
    }

    async fn step(&mut self) -> Result<StepAction> {
        if !self.started {
            return Ok(StepAction::AwaitEvent);
        }

        if self.remaining == 0 {
            return Ok(StepAction::Never);
        }

        while self.remaining > 0 {
            self.ctx
                .send(BenchEvent::Payload(self.payload_template.clone()))
                .await?;
            self.remaining -= 1;
        }

        Ok(StepAction::Never)
    }
}

/// Benchmark consumer actor.
/// It counts received messages and signals completion once `target` is reached.
struct Consumer {
    received: usize,
    target: usize,
    done_tx: Option<oneshot::Sender<()>>,
}

impl Actor for Consumer {
    type Event = BenchEvent;

    async fn handle_event(&mut self, envelope: &Envelope<Self::Event>) -> Result {
        match envelope.event() {
            BenchEvent::Start => {}
            BenchEvent::Payload(payload) => {
                black_box(payload.len());
            }
        }

        self.received += 1;
        if self.received == self.target
            && let Some(done_tx) = self.done_tx.take()
        {
            let _ = done_tx.send(());
        }

        Ok(())
    }
}

async fn run_round(message_count: usize, payload_size: usize) -> Duration {
    let (done_tx, done_rx) = oneshot::channel::<()>();
    let mut done_tx = Some(done_tx);
    let payload_template = vec![0_u8; payload_size];

    let mut supervisor = Supervisor::<BenchEvent, BenchTopic>::default();
    supervisor
        .add_actor(
            "producer",
            move |ctx| Producer {
                ctx,
                started: false,
                remaining: message_count,
                payload_template,
            },
            [BenchTopic::Control],
        )
        .expect("producer registration should succeed");

    supervisor
        .build_actor("consumer", move |_ctx| Consumer {
            received: 0,
            target: message_count,
            done_tx: done_tx.take(),
        })
        .topics(&[BenchTopic::Payload])
        .with_config(|cfg| cfg.with_max_events_per_tick(message_count))
        .build()
        .expect("consumer registration should succeed");

    supervisor.start().await.expect("supervisor should start");

    let start = Instant::now();
    supervisor
        .send(BenchEvent::Start)
        .await
        .expect("start signal should be delivered");
    done_rx
        .await
        .expect("consumer should signal completion for each round");
    let elapsed = start.elapsed();

    supervisor
        .stop()
        .await
        .expect("supervisor stop should succeed");

    elapsed
}

fn bench_actor_to_actor_transport(c: &mut Criterion) {
    let runtime = Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime should build");

    let mut group = c.benchmark_group("actor_to_actor_transport_maiko");
    group.sample_size(10);
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(1));

    let message_counts = [1_000_usize, 10_000, 100_000];
    let payload_sizes = [32_usize, 256, 1_024, 4_096];

    for &message_count in &message_counts {
        for &payload_size in &payload_sizes {
            group.throughput(Throughput::Elements(message_count as u64));
            group.bench_with_input(
                BenchmarkId::new(format!("{payload_size}B"), message_count),
                &(message_count, payload_size),
                |b, &(message_count, payload_size)| {
                    b.iter_custom(|iters| {
                        runtime.block_on(async {
                            let mut total = Duration::ZERO;
                            for _ in 0..iters {
                                total += run_round(message_count, payload_size).await;
                            }
                            total
                        })
                    });
                },
            );
        }
    }

    group.finish();
}

criterion_group!(benches, bench_actor_to_actor_transport);
criterion_main!(benches);
