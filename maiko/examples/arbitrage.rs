//! Arbitrage Trading Example - Test Harness Demonstration
//!
//! This example demonstrates the Maiko test harness for integration testing
//! of actor systems. It simulates a simple arbitrage trading system with:
//!
//! - **Ticker actors**: Receive orders (simulated exchange connections)
//! - **Normalizer actor**: Converts raw tick formats to a common format
//! - **Trader actor**: Detects arbitrage opportunities and places orders
//! - **Database actor**: Would persist data (stubbed)
//! - **Telemetry actor**: Observes all events for monitoring
//!
//! The test harness allows us to:
//! - Inject events and observe their propagation
//! - Assert on event delivery to specific actors
//! - Query events by sender, receiver, topic, or custom predicates
//! - Track parent-child event relationships via parent IDs

use maiko::{Actor, Context, Subscribe, Topic, testing::Harness};

// ============================================================================
// Domain Types
// ============================================================================

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
enum Exchange {
    Alpha,
    Beta,
}

#[derive(Clone, Debug)]
enum Side {
    Buy,
    Sell,
}

#[derive(Clone, Debug)]
#[allow(unused)]
struct Tick {
    ex: Exchange,
    price: f64,
    size: usize,
    side: Side,
}

impl Tick {
    fn new(ex: Exchange, price: f64, size: usize, side: Side) -> Self {
        Self {
            ex,
            price,
            size,
            side,
        }
    }
}

#[derive(Clone, Default)]
struct TopOfBook {
    bid: f64,
    ask: f64,
}

// ============================================================================
// Events and Topics
// ============================================================================

#[derive(maiko::Event, Clone, Debug)]
enum MarketEvent {
    /// Raw tick from Alpha exchange
    AlphaTick { price: f64, quantity: u32 },
    /// Raw tick from Beta exchange
    BetaTick {
        price: String,
        side: char,
        count: u64,
    },
    /// Normalized tick (common format)
    MarketTick(Tick),
    /// Order to be executed
    Order(Tick),
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
enum MarketTopic {
    RawData,
    NormalizedData,
    Order(Exchange),
}

impl Topic<MarketEvent> for MarketTopic {
    fn from_event(event: &MarketEvent) -> Self {
        use MarketEvent::*;
        match event {
            AlphaTick { .. } | BetaTick { .. } => MarketTopic::RawData,
            MarketTick(_) => MarketTopic::NormalizedData,
            Order(tick) => MarketTopic::Order(tick.ex.clone()),
        }
    }
}

// ============================================================================
// Actors
// ============================================================================

/// Ticker actor - receives orders for an exchange (stub)
struct Ticker;
impl Actor for Ticker {
    type Event = MarketEvent;
}

/// Normalizer - converts raw ticks to common format
struct Normalizer {
    ctx: Context<MarketEvent>,
}

impl Actor for Normalizer {
    type Event = MarketEvent;

    async fn handle_event(&mut self, envelope: &maiko::Envelope<Self::Event>) -> maiko::Result<()> {
        let tick = match envelope.event() {
            MarketEvent::AlphaTick { price, quantity } => {
                Tick::new(Exchange::Alpha, *price, *quantity as usize, Side::Buy)
            }
            MarketEvent::BetaTick { price, side, count } => {
                let side_enum = if *side == 'B' { Side::Buy } else { Side::Sell };
                let price_f64 = price.parse::<f64>().unwrap_or(0.0);
                Tick::new(Exchange::Beta, price_f64, *count as usize, side_enum)
            }
            _ => return Ok(()),
        };

        // Send normalized tick as a child event (correlated to parent)
        self.ctx
            .send_child_event(MarketEvent::MarketTick(tick), envelope.id())
            .await
    }
}

/// Trader - detects arbitrage and places orders
struct Trader {
    ctx: Context<MarketEvent>,
    alpha_top: Option<TopOfBook>,
    beta_top: Option<TopOfBook>,
}

impl Trader {
    fn new(ctx: Context<MarketEvent>) -> Self {
        Self {
            ctx,
            alpha_top: None,
            beta_top: None,
        }
    }

    fn update_book(&mut self, tick: &Tick) {
        let top = match tick.ex {
            Exchange::Alpha => self.alpha_top.get_or_insert_default(),
            Exchange::Beta => self.beta_top.get_or_insert_default(),
        };
        match tick.side {
            Side::Buy => top.bid = tick.price,
            Side::Sell => top.ask = tick.price,
        }
    }

    fn check_arbitrage(&self) -> Option<(Exchange, f64)> {
        match (&self.alpha_top, &self.beta_top) {
            (Some(alpha), Some(beta)) if alpha.ask > 0.0 && beta.bid > alpha.ask => {
                Some((Exchange::Alpha, alpha.ask))
            }
            (Some(alpha), Some(beta)) if beta.ask > 0.0 && alpha.bid > beta.ask => {
                Some((Exchange::Beta, beta.ask))
            }
            _ => None,
        }
    }
}

impl Actor for Trader {
    type Event = MarketEvent;

    async fn handle_event(&mut self, envelope: &maiko::Envelope<Self::Event>) -> maiko::Result<()> {
        if let MarketEvent::MarketTick(tick) = envelope.event() {
            self.update_book(tick);

            if let Some((buy_ex, price)) = self.check_arbitrage() {
                let sell_ex = match buy_ex {
                    Exchange::Alpha => Exchange::Beta,
                    Exchange::Beta => Exchange::Alpha,
                };

                // Place both legs of the arbitrage trade
                self.ctx
                    .send(MarketEvent::Order(Tick::new(buy_ex, price, 100, Side::Buy)))
                    .await?;
                self.ctx
                    .send(MarketEvent::Order(Tick::new(
                        sell_ex,
                        price,
                        100,
                        Side::Sell,
                    )))
                    .await?;
            }
        }
        Ok(())
    }
}

/// Database actor (stub)
struct Database;
impl Actor for Database {
    type Event = MarketEvent;
}

/// Telemetry actor - observes all events
struct Telemetry;
impl Actor for Telemetry {
    type Event = MarketEvent;
}

// ============================================================================
// Test Harness Demonstration
// ============================================================================

#[tokio::main]
async fn main() -> maiko::Result {
    use Exchange::*;
    use MarketTopic::*;

    // Set up the actor system
    let mut sup = maiko::Supervisor::<MarketEvent, MarketTopic>::default();

    let alpha_ticker = sup.add_actor("AlphaTicker", |_| Ticker, [Order(Alpha)])?;
    let beta_ticker = sup.add_actor("BetaTicker", |_| Ticker, [Order(Beta)])?;
    let normalizer = sup.add_actor("Normalizer", |ctx| Normalizer { ctx }, [RawData])?;
    let trader = sup.add_actor("Trader", Trader::new, [NormalizedData])?;
    let _database = sup.add_actor("Database", |_| Database, [NormalizedData])?;
    let telemetry = sup.add_actor("Telemetry", |_| Telemetry, Subscribe::all())?;

    // Initialize test harness
    let mut test = Harness::new(&mut sup).await;
    sup.start().await?;

    println!("=== Test Harness Demonstration ===\n");

    // ========================================================================
    // Test 1: Basic Event Delivery
    // ========================================================================
    println!("--- Test 1: Basic Event Delivery ---");

    test.record().await;
    let tick_id = test
        .send_as(
            &alpha_ticker,
            MarketEvent::AlphaTick {
                price: 100.0,
                quantity: 50,
            },
        )
        .await?;
    test.settle().await;

    // Use EventSpy to inspect the sent event
    let event_spy = test.event(tick_id);
    assert!(event_spy.was_delivered(), "Event should be delivered");
    assert!(
        event_spy.was_delivered_to_all(&[&normalizer, &telemetry]),
        "Should reach both Normalizer and Telemetry"
    );
    assert_eq!(2, event_spy.receiver_count(), "Should have 2 receivers");

    println!("  AlphaTick delivered to: {:?}", event_spy.receivers());
    println!(
        "  Child events (correlated): {}",
        event_spy.children().count()
    );

    // ========================================================================
    // Test 2: Actor Perspective
    // ========================================================================
    println!("\n--- Test 2: Actor Perspective (ActorSpy) ---");

    let normalizer_spy = test.actor(&normalizer);
    println!(
        "  Normalizer inbound count: {}",
        normalizer_spy.events_received()
    );
    println!(
        "  Normalizer outbound count: {}",
        normalizer_spy.events_sent()
    );
    println!(
        "  Normalizer received from: {:?}",
        normalizer_spy.received_from()
    );
    println!("  Normalizer sent to: {:?}", normalizer_spy.sent_to());

    assert_eq!(
        1,
        normalizer_spy.events_received(),
        "Normalizer should receive 1 event"
    );
    assert_eq!(
        1,
        normalizer_spy.events_sent(),
        "Normalizer should send 1 event"
    );

    let telemetry_spy = test.actor(&telemetry);
    println!(
        "  Telemetry inbound count: {}",
        telemetry_spy.events_received()
    );
    println!(
        "  Telemetry received from: {:?}",
        telemetry_spy.received_from()
    );
    assert_eq!(
        0,
        telemetry_spy.events_sent(),
        "Telemetry is passive (no outbound)"
    );

    // ========================================================================
    // Test 3: Topic Inspection
    // ========================================================================
    println!("\n--- Test 3: Topic Inspection (TopicSpy) ---");

    let raw_topic = test.topic(RawData);
    println!("  RawData event count: {}", raw_topic.event_count());
    println!("  RawData receivers: {:?}", raw_topic.receivers());

    let normalized_topic = test.topic(NormalizedData);
    println!(
        "  NormalizedData event count: {}",
        normalized_topic.event_count()
    );

    // ========================================================================
    // Test 4: EventQuery for Complex Queries
    // ========================================================================
    println!("\n--- Test 4: Complex Queries (EventQuery) ---");

    // Find all events sent by Normalizer
    let normalizer_events = test.events().sent_by(&normalizer).count();
    println!("  Events sent by Normalizer: {}", normalizer_events);

    // Get unique senders and receivers across all events
    let senders = test.events().senders();
    let receivers = test.events().receivers();
    println!("  Unique senders: {:?}", senders);
    println!("  Unique receivers: {:?}", receivers);

    // Find all MarketTick events
    let market_ticks = test
        .events()
        .matching_event(|e| matches!(e, MarketEvent::MarketTick(_)))
        .count();
    println!("  MarketTick events: {}", market_ticks);

    // Chain multiple filters
    let trader_received_from_normalizer = test
        .events()
        .sent_by(&normalizer)
        .received_by(&trader)
        .count();
    println!(
        "  Trader received from Normalizer: {}",
        trader_received_from_normalizer
    );

    // ========================================================================
    // Test 5: Arbitrage Scenario
    // ========================================================================
    println!("\n--- Test 5: Full Arbitrage Scenario ---");

    test.reset();
    test.record().await;

    // Send ticks that will trigger arbitrage
    // Alpha: ask at 100
    test.send_as(
        &alpha_ticker,
        MarketEvent::AlphaTick {
            price: 100.0,
            quantity: 100,
        },
    )
    .await?;

    // Beta: bid at 105 (higher than Alpha ask = arbitrage opportunity!)
    test.send_as(
        &beta_ticker,
        MarketEvent::BetaTick {
            price: "105.0".to_string(),
            side: 'B',
            count: 100,
        },
    )
    .await?;

    test.settle().await;

    // Check if orders were generated
    let alpha_orders = test.topic(Order(Alpha));
    let beta_orders = test.topic(Order(Beta));

    println!("  Alpha orders: {}", alpha_orders.event_count());
    println!("  Beta orders: {}", beta_orders.event_count());

    // Verify trader's activity
    let trader_spy = test.actor(&trader);
    println!("  Trader inbound: {}", trader_spy.events_received());
    println!("  Trader outbound: {}", trader_spy.events_sent());

    // Dump all events for debugging
    println!("\n--- Event Dump ---");
    test.dump();

    // ========================================================================
    // Test 6: Using EventQuery from spies
    // ========================================================================
    println!("\n--- Test 6: Chaining from Spies ---");

    // Get a query from a spy and further filter it
    let trader_outbound = test.actor(&trader).outbound();
    let buy_orders = trader_outbound
        .matching_event(|e| matches!(e, MarketEvent::Order(t) if matches!(t.side, Side::Buy)))
        .count();
    println!("  Trader buy orders: {}", buy_orders);

    // Clean up
    sup.stop().await?;

    println!("\n=== All Tests Passed ===");
    Ok(())
}
