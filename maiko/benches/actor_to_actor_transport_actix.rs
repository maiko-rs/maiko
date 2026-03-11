//! Benchmarks end-to-end Actor -> Actor transport throughput in `actix`.
//!
//! Matrix:
//! - message count: 1_000, 10_000, 100_000
//! - payload size: 32B, 256B, 1KiB, 4KiB
//!
//! Throughput unit is messages/sec.
//!
//! Run with:
//! `cargo bench -p maiko --bench actor_to_actor_transport_actix -- --noplot`

use std::{hint::black_box, time::Duration};

use actix::prelude::*;
use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use tokio::{sync::oneshot, time::Instant};

#[derive(Message)]
#[rtype(result = "()")]
struct BenchMessage(Vec<u8>);

#[derive(Message)]
#[rtype(result = "()")]
struct Start;

/// Benchmark producer actor.
/// It sends `remaining` payload messages as fast as possible after `Start`.
struct Producer {
    consumer: Addr<Consumer>,
    remaining: usize,
    payload_template: Vec<u8>,
}

impl Actor for Producer {
    type Context = Context<Self>;
}

impl Handler<Start> for Producer {
    type Result = ();

    fn handle(&mut self, _message: Start, ctx: &mut Self::Context) -> Self::Result {
        while self.remaining > 0 {
            self.consumer
                .do_send(BenchMessage(self.payload_template.clone()));
            self.remaining -= 1;
        }
        ctx.stop();
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
    type Context = Context<Self>;
}

impl Handler<BenchMessage> for Consumer {
    type Result = ();

    fn handle(&mut self, message: BenchMessage, ctx: &mut Self::Context) -> Self::Result {
        black_box(message.0.len());
        self.received += 1;
        if self.received == self.target {
            if let Some(done_tx) = self.done_tx.take() {
                let _ = done_tx.send(());
            }
            ctx.stop();
        }
    }
}

fn run_round(message_count: usize, payload_size: usize) -> Duration {
    let system = System::new();
    system.block_on(async move {
        let (done_tx, done_rx) = oneshot::channel::<()>();
        let payload_template = vec![0_u8; payload_size];

        let consumer = Consumer {
            received: 0,
            target: message_count,
            done_tx: Some(done_tx),
        }
        .start();

        let producer = Producer {
            consumer,
            remaining: message_count,
            payload_template,
        }
        .start();

        let start = Instant::now();
        producer.do_send(Start);
        done_rx
            .await
            .expect("consumer should signal completion for each round");
        start.elapsed()
    })
}

fn bench_actor_to_actor_transport_actix(c: &mut Criterion) {
    let mut group = c.benchmark_group("actor_to_actor_transport_actix");
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
                        let mut total = Duration::ZERO;
                        for _ in 0..iters {
                            total += run_round(message_count, payload_size);
                        }
                        total
                    });
                },
            );
        }
    }

    group.finish();
}

criterion_group!(benches, bench_actor_to_actor_transport_actix);
criterion_main!(benches);
