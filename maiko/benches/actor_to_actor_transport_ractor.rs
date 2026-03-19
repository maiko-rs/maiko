//! Benchmarks end-to-end Actor -> Actor transport throughput in `ractor`.
//!
//! Matrix:
//! - message count: 1_000, 10_000, 100_000
//! - payload size: 32B, 256B, 1KiB, 4KiB
//!
//! Throughput unit is messages/sec.
//!
//! Run with:
//! `cargo bench -p maiko --bench actor_to_actor_transport_ractor -- --noplot`

use std::{hint::black_box, time::Duration};

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use ractor::{Actor, ActorProcessingErr, ActorRef, async_trait};
use tokio::{runtime::Builder, sync::oneshot, time::Instant};

#[derive(Clone)]
enum BenchMessage {
    Payload(Vec<u8>),
}

enum ProducerMessage {
    Start,
}

/// Benchmark consumer actor state.
struct ConsumerState {
    received: usize,
    target: usize,
    done_tx: Option<oneshot::Sender<()>>,
}

/// Benchmark consumer actor.
/// It counts received messages and signals completion once `target` is reached.
struct Consumer;

#[async_trait]
impl Actor for Consumer {
    type Msg = BenchMessage;
    type State = ConsumerState;
    type Arguments = (usize, oneshot::Sender<()>);

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        (target, done_tx): Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        Ok(ConsumerState {
            received: 0,
            target,
            done_tx: Some(done_tx),
        })
    }

    async fn handle(
        &self,
        _myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            BenchMessage::Payload(payload) => {
                black_box(payload.len());
            }
        }

        state.received += 1;
        if state.received == state.target
            && let Some(done_tx) = state.done_tx.take()
        {
            let _ = done_tx.send(());
        }

        Ok(())
    }
}

/// Benchmark producer actor.
/// It sends `remaining` payload messages as fast as possible after `ProducerMessage::Start`.
struct Producer;

struct ProducerState {
    consumer: ActorRef<BenchMessage>,
    remaining: usize,
    payload_template: Vec<u8>,
}

#[async_trait]
impl Actor for Producer {
    type Msg = ProducerMessage;
    type State = ProducerState;
    type Arguments = (ActorRef<BenchMessage>, usize, Vec<u8>);

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        (consumer, remaining, payload_template): Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        Ok(ProducerState {
            consumer,
            remaining,
            payload_template,
        })
    }

    async fn handle(
        &self,
        _myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            ProducerMessage::Start => {
                for _ in 0..state.remaining {
                    state
                        .consumer
                        .send_message(BenchMessage::Payload(state.payload_template.clone()))?;
                }
                state.remaining = 0;
            }
        }
        Ok(())
    }
}

async fn run_round(message_count: usize, payload_size: usize) -> Duration {
    let (done_tx, done_rx) = oneshot::channel::<()>();
    let payload_template = vec![0_u8; payload_size];

    let (consumer_ref, consumer_handle) = Actor::spawn(None, Consumer, (message_count, done_tx))
        .await
        .expect("consumer spawn should succeed");
    let (producer_ref, producer_handle) = Actor::spawn(
        None,
        Producer,
        (consumer_ref.clone(), message_count, payload_template),
    )
    .await
    .expect("producer spawn should succeed");

    let start = Instant::now();
    producer_ref
        .send_message(ProducerMessage::Start)
        .expect("producer should accept start signal");
    done_rx
        .await
        .expect("consumer should signal completion for each round");
    let elapsed = start.elapsed();

    producer_ref.stop(None);
    consumer_ref.stop(None);
    producer_handle
        .await
        .expect("producer should terminate without panic");
    consumer_handle
        .await
        .expect("consumer should terminate without panic");

    elapsed
}

fn bench_actor_to_actor_transport_ractor(c: &mut Criterion) {
    let runtime = Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime should build");

    let mut group = c.benchmark_group("actor_to_actor_transport_ractor");
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

criterion_group!(benches, bench_actor_to_actor_transport_ractor);
criterion_main!(benches);
