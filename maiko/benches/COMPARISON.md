# Actor-to-Actor Comparison

This directory contains three comparative actor-to-actor transport benchmarks:

- `actor_to_actor_transport_maiko.rs`
- `actor_to_actor_transport_actix.rs`
- `actor_to_actor_transport_ractor.rs`

They all benchmark the same matrix:

- message counts: `1_000`, `10_000`, `100_000`
- payload sizes: `32B`, `256B`, `1KiB`, `4KiB`

Criterion reports throughput in messages/sec.

## Important framing

This comparison is useful, but it is not perfectly apples-to-apples.

`actix` and `ractor` are being exercised here as direct actor-to-actor transport systems. Maiko is not. Maiko routes messages through a broker, which means delivery is fundamentally a two-step path:

1. producer -> broker
2. broker -> subscriber

That architectural choice is deliberate. It gives Maiko topic-based routing, looser coupling, and a more detached event-driven model, but it also means Maiko should not be expected to compete with direct-message runtimes on raw actor-to-actor throughput.

Relatedly, Maiko is unlikely to reach `1M` events/sec in this style of benchmark, and that is not a project goal. Maiko is optimized primarily for ergonomics and a detached architecture. Pure transport performance is secondary.

## How to run

Run the full comparison from the repository root:

```sh
just bench-a2a-compare
```

That target executes:

```sh
cargo bench --profile bench -p maiko --bench actor_to_actor_transport_maiko -- --noplot
cargo bench --profile bench -p maiko --bench actor_to_actor_transport_actix -- --noplot
cargo bench --profile bench -p maiko --bench actor_to_actor_transport_ractor -- --noplot
uv run --project scripts scripts/bench_a2a_compare.py
```

If you want to run the benches individually:

```sh
cargo bench --profile bench -p maiko --bench actor_to_actor_transport_maiko -- --noplot
cargo bench --profile bench -p maiko --bench actor_to_actor_transport_actix -- --noplot
cargo bench --profile bench -p maiko --bench actor_to_actor_transport_ractor -- --noplot
```

## Backpressure notes

Backpressure matters here because Maiko's transport semantics are intentionally bounded and blocking under load. A comparison that ignores that entirely for the other runtimes is easier to implement, but it stops being a like-for-like transport comparison.

### Maiko

Maiko's benchmark uses real backpressure:

- the producer sends via `Context::send(...).await`
- the payload topic uses `OverflowPolicy::Block`

That means the producer naturally waits when downstream capacity is exhausted.

### Actix

Actix also uses an explicit backpressure path in this comparison:

- the producer sends via `Addr::send(...).await`
- the consumer mailbox is explicitly bounded with `ctx.set_mailbox_capacity(MAILBOX_CAPACITY)`

This is the closest Actix analogue to Maiko's blocking send semantics. It is not a perfect apples-to-apples mapping, because Actix `send` is request/response-oriented and carries reply-channel overhead even when the result type is `()`, but it does provide actual capacity-coupled waiting.

### Ractor

No backpressure is implemented in the `ractor` benchmark.

The benchmark currently uses `ActorRef::send_message(...)`, which is a simple push path with no await-based mailbox backpressure. We explored replacing it with factory-based admission control, but that path appears substantially more cumbersome than the other two implementations and is not strictly necessary for keeping the comparative benchmark useful. Because of that, the `ractor` benchmark intentionally remains a simpler no-backpressure transport benchmark for now.

In short:

- Maiko: backpressure enabled
- Actix: backpressure enabled
- Ractor: backpressure intentionally not implemented

## Example results

These are example comparison results from one benchmark run.

### Absolute throughput

```text
╭─────────────┬───────────────┬───────────┬───────────┬───────────┬───────────╮
│   Framework │   N \ Payload │       32B │      256B │      1KiB │      4KiB │
├─────────────┼───────────────┼───────────┼───────────┼───────────┼───────────┤
│       maiko │         1,000 │   813,976 │   822,254 │   773,352 │   720,358 │
│       maiko │        10,000 │   812,887 │   812,014 │   768,516 │   643,479 │
│       maiko │       100,000 │   778,227 │   782,469 │   712,197 │   713,271 │
│       actix │         1,000 │ 4,072,205 │ 4,182,926 │ 3,710,988 │ 3,411,817 │
│       actix │        10,000 │ 4,292,594 │ 4,373,233 │ 3,939,233 │ 3,080,245 │
│       actix │       100,000 │ 4,276,946 │ 4,197,778 │ 3,911,589 │ 3,130,537 │
│      ractor │         1,000 │ 3,194,561 │ 3,192,353 │ 2,692,416 │ 2,112,884 │
│      ractor │        10,000 │ 3,012,807 │ 3,013,401 │ 2,571,302 │ 1,688,873 │
│      ractor │       100,000 │ 3,069,069 │ 2,843,617 │ 2,434,496 │ 1,702,354 │
╰─────────────┴───────────────┴───────────┴───────────┴───────────┴───────────╯
```

Unit: messages/sec

### Relative throughput vs Maiko

```text
╭─────────────┬───────────────┬───────┬────────┬────────┬────────╮
│   Framework │   N \ Payload │   32B │   256B │   1KiB │   4KiB │
├─────────────┼───────────────┼───────┼────────┼────────┼────────┤
│       actix │         1,000 │ 5.00x │  5.09x │  4.80x │  4.74x │
│       actix │        10,000 │ 5.28x │  5.39x │  5.13x │  4.79x │
│       actix │       100,000 │ 5.50x │  5.36x │  5.49x │  4.39x │
│      ractor │         1,000 │ 3.92x │  3.88x │  3.48x │  2.93x │
│      ractor │        10,000 │ 3.71x │  3.71x │  3.35x │  2.62x │
│      ractor │       100,000 │ 3.94x │  3.63x │  3.42x │  2.39x │
╰─────────────┴───────────────┴───────┴────────┴────────┴────────╯
```

Unit: multiplier (higher is better)

## Reading the results

From the sample run:

- `actix` is the fastest implementation across the full matrix, roughly `4.4x` to `5.5x` faster than Maiko.
- `ractor` is consistently faster than Maiko as well, roughly `2.4x` to `3.9x` faster in this run.
- both `actix` and `ractor` degrade as payload size increases, but `ractor` drops off more sharply at `4KiB`.
- Maiko is the slowest of the three in raw throughput, but it is also the implementation whose benchmark most directly reflects its bounded, blocking pub/sub transport semantics.
- Maiko should therefore be read here as "brokered event transport with stronger architectural separation", not as "direct actor messaging tuned for maximum throughput".

These numbers should be read as transport benchmark results for the specific benchmark harnesses in this directory, not as universal claims about each framework in all workloads.
