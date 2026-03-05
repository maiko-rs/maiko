//! Benchmarks subscriber lookup strategies for fictional broker layouts.
//!
//! This compares a linear `Vec<Subscriber<...>>` scan against a topic-indexed
//! `HashMap` layout using `fetch_subscribers` lookups based on
//! `T::from_event(e.event())`.
//!
//! The benchmark is intended to estimate when topic indexing outperforms full
//! scans as subscriber counts grow. It measures lookup cost only, not runtime
//! send/backpressure behavior or index maintenance overhead.
//!
//! Run it `cargo bench -p maiko --bench broker_subscriber_lookup`.
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use maiko::{ActorId, Envelope, Event, Topic};
use std::hash::Hash;
use std::hint::black_box;

use std::{
    collections::{HashMap, HashSet},
    marker::PhantomData,
};

const TOPIC_COUNT: usize = 32;
const TOPICS_PER_SUBSCRIBER: usize = 3;

#[derive(Debug, Clone)]
struct BenchEvent {
    topic: BenchTopic,
    _payload: u64,
}

impl Event for BenchEvent {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct BenchTopic(u16);

impl Topic<BenchEvent> for BenchTopic {
    fn from_event(event: &BenchEvent) -> Self {
        event.topic
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Subscription<T: Eq + Hash> {
    All,
    Topics(HashSet<T>),
    None,
}

impl<T: Eq + Hash> Subscription<T> {
    fn contains(&self, topic: &T) -> bool {
        match self {
            Subscription::All => true,
            Subscription::Topics(topics) => topics.contains(topic),
            Subscription::None => false,
        }
    }
}

#[derive(Debug, Clone)]
struct Subscriber<E: Event, T: Topic<E>> {
    actor_id: ActorId,
    topics: Subscription<T>,
    _event: PhantomData<E>,
}

impl<E: Event, T: Topic<E>> Subscriber<E, T> {
    fn new(actor_id: ActorId, topics: Subscription<T>) -> Self {
        Self {
            actor_id,
            topics,
            _event: PhantomData,
        }
    }
}

struct BrokerVec<E: Event, T: Topic<E>> {
    subscribers: Vec<Subscriber<E, T>>,
}

impl<E: Event, T: Topic<E>> BrokerVec<E, T> {
    fn new() -> Self {
        Self {
            subscribers: Vec::new(),
        }
    }

    fn add_subscriber(&mut self, subscriber: Subscriber<E, T>) {
        self.subscribers.push(subscriber);
    }

    fn fetch_subscribers(&self, e: &Envelope<E>) -> usize {
        let topic = T::from_event(e.event());

        self.subscribers
            .iter()
            .filter(|subscriber| subscriber.topics.contains(&topic))
            .count()
    }
}

struct BrokerHashMap<E: Event, T: Topic<E>> {
    subscribers: HashMap<ActorId, Subscriber<E, T>>,
    subscribers_by_topic: HashMap<T, Vec<ActorId>>,
    subscribers_for_all: Vec<ActorId>,
}

impl<E: Event, T: Topic<E>> BrokerHashMap<E, T> {
    fn new() -> Self {
        Self {
            subscribers: HashMap::new(),
            subscribers_by_topic: HashMap::new(),
            subscribers_for_all: Vec::new(),
        }
    }

    fn add_subscriber(&mut self, subscriber: Subscriber<E, T>) {
        let actor_id = subscriber.actor_id.clone();

        match &subscriber.topics {
            Subscription::All => self.subscribers_for_all.push(actor_id.clone()),
            Subscription::Topics(topics) => {
                for topic in topics {
                    self.subscribers_by_topic
                        .entry(topic.clone())
                        .or_default()
                        .push(actor_id.clone());
                }
            }
            Subscription::None => {}
        }

        self.subscribers.insert(actor_id, subscriber);
    }

    fn fetch_subscribers(&self, e: &Envelope<E>) -> usize {
        let topic = T::from_event(e.event());
        let mut count = 0usize;

        if let Some(actor_ids) = self.subscribers_by_topic.get(&topic) {
            for actor_id in actor_ids {
                if self.subscribers.contains_key(actor_id) {
                    count += 1;
                }
            }
        }

        for actor_id in &self.subscribers_for_all {
            if self.subscribers.contains_key(actor_id) {
                count += 1;
            }
        }

        count
    }
}

fn create_subscription(subscriber_index: usize) -> Subscription<BenchTopic> {
    if subscriber_index % 10 == 0 {
        return Subscription::All;
    }

    if subscriber_index % 11 == 0 {
        return Subscription::None;
    }

    let topics = (0..TOPICS_PER_SUBSCRIBER)
        .map(|offset| BenchTopic(((subscriber_index + offset) % TOPIC_COUNT) as u16))
        .collect();
    Subscription::Topics(topics)
}

#[allow(clippy::type_complexity)] // Not important for the bench.
fn build_fictional_brokers(
    subscriber_count: usize,
) -> (
    BrokerVec<BenchEvent, BenchTopic>,
    BrokerHashMap<BenchEvent, BenchTopic>,
    Vec<Envelope<BenchEvent>>,
) {
    let mut broker_vec = BrokerVec::<BenchEvent, BenchTopic>::new();
    let mut broker_hash_map = BrokerHashMap::<BenchEvent, BenchTopic>::new();

    for subscriber_index in 0..subscriber_count {
        let actor_id = ActorId::from(format!("actor-{subscriber_index}"));
        let topics = create_subscription(subscriber_index);
        let subscriber = Subscriber::<BenchEvent, BenchTopic>::new(actor_id, topics);

        broker_vec.add_subscriber(subscriber.clone());
        broker_hash_map.add_subscriber(subscriber);
    }

    let source = ActorId::new("benchmark-source");
    let envelopes = (0..TOPIC_COUNT)
        .map(|topic| {
            Envelope::new(
                BenchEvent {
                    topic: BenchTopic(topic as u16),
                    _payload: topic as u64,
                },
                source.clone(),
            )
        })
        .collect::<Vec<_>>();

    for envelope in &envelopes {
        assert_eq!(
            broker_vec.fetch_subscribers(envelope),
            broker_hash_map.fetch_subscribers(envelope)
        );
    }

    (broker_vec, broker_hash_map, envelopes)
}

fn bench_broker_subscriber_lookup(c: &mut Criterion) {
    let mut group = c.benchmark_group("broker_subscriber_lookup");

    for subscriber_count in [5usize, 10, 25, 50, 100, 250, 500, 1_000, 2_000] {
        let (broker_vec, broker_hash_map, envelopes) = build_fictional_brokers(subscriber_count);

        group.bench_with_input(
            BenchmarkId::new("vec_scan", subscriber_count),
            &subscriber_count,
            |b, _| {
                b.iter(|| {
                    let mut matched_subscribers = 0usize;
                    for envelope in black_box(&envelopes) {
                        matched_subscribers += broker_vec.fetch_subscribers(black_box(envelope));
                    }
                    black_box(matched_subscribers)
                })
            },
        );

        group.bench_with_input(
            BenchmarkId::new("hash_map_index", subscriber_count),
            &subscriber_count,
            |b, _| {
                b.iter(|| {
                    let mut matched_subscribers = 0usize;
                    for envelope in black_box(&envelopes) {
                        matched_subscribers +=
                            broker_hash_map.fetch_subscribers(black_box(envelope));
                    }
                    black_box(matched_subscribers)
                })
            },
        );
    }

    group.finish();
}

criterion_group!(benches, bench_broker_subscriber_lookup);
criterion_main!(benches);
