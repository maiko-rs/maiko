//! Guessing Game - Actor Coordination Example
//!
//! The Game waits for guesses from both players, pairs them, and emits results.
//!
//! This example demonstrates Maiko patterns for coordinating multiple actors
//! in a stateful, interactive system.
//!
//! ## 1. Tick-Based Actors (Event Producers)
//!
//! The `Player` actors use `tick()` as their primary logic - they don't react to events,
//! they **generate** them at regular intervals.
//!
//! ## 2. Stateful Coordination
//!
//! The `Game` actor maintains state across multiple events, collecting guesses from
//! multiple players before processing them together. This shows how actors can
//! coordinate without knowing about each other - they just emit and consume events.
//!
//! ## 3. Topic-Based Separation of Concerns
//!
//! Events are routed to different topics based on their purpose:
//! - **Game topic**: Game logic events (guesses) go to the Game coordinator
//! - **Output topic**: Display events (results, messages) go to the Printer
//!
//! This separation allows clean architectural boundaries without coupling actors.
//!
//! ## 4. Self-Termination
//!
//! The Game actor stops the entire runtime using `ctx.stop_runtime()` after completing its task.

use std::sync::Arc;
use std::time::Duration;

use maiko::*;

/// Player identifier for distinguishing events from different players.
///
/// By encoding the player ID in the event itself (rather than using actor names),
/// we maintain type safety and avoid string comparisons.
#[derive(Clone, Debug, Copy, PartialEq, Eq)]
enum PlayerId {
    Player1,
    Player2,
}

/// Events in the guessing game.
///
/// Each event carries structured data specific to its purpose:
/// - `Guess`: Contains both the player ID and their guess
/// - `Result`: Contains both players' numbers for comparison
/// - `Message`: Carries string output for display
#[derive(Clone, Debug, Event)]
enum GuesserEvent {
    Guess { player: PlayerId, number: u8 },
    Result { player1: u8, player2: u8 },
    Message(String),
}

/// Topics for routing events to appropriate actors.
///
/// Game logic events go to the coordinator, while output events go to the printer.
/// This separation allows clean architectural boundaries.
#[derive(Debug, Hash, Eq, PartialEq, Clone)]
enum GuesserTopic {
    Game,
    Output,
}

impl Topic<GuesserEvent> for GuesserTopic {
    fn from_event(event: &GuesserEvent) -> Self {
        use GuesserEvent::*;
        use GuesserTopic::*;

        match event {
            Message(..) => Output,
            Result { .. } => Output,
            Guess { .. } => Game,
        }
    }
}

/// A player actor that generates guesses at regular intervals.
///
/// This is a **tick-based actor** - its primary logic is in `tick()` rather than
/// `handle()`. Players don't react to events; they produce them on a schedule.
struct Guesser {
    ctx: Context<GuesserEvent>,
    player_id: PlayerId,
    cycle_time: Duration,
}

impl Guesser {
    fn new(ctx: Context<GuesserEvent>, player_id: PlayerId, interval_ms: u64) -> Self {
        Self {
            ctx,
            player_id,
            cycle_time: Duration::from_millis(interval_ms),
        }
    }
}

impl Actor for Guesser {
    type Event = GuesserEvent;

    /// Generate a random guess at regular intervals.
    async fn step(&mut self) -> maiko::Result<StepAction> {
        let rand = getrandom::u32().map_err(|e| Error::External(Arc::new(e)))?;
        let number = (rand % 10) as u8;

        // Emit a guess event with our player ID
        self.ctx
            .send(GuesserEvent::Guess {
                player: self.player_id,
                number,
            })
            .await?;

        Ok(StepAction::Backoff(self.cycle_time))
    }
}

/// The game coordinator that pairs guesses and determines results.
///
/// This is a **stateful actor** that:
/// - Collects guesses from multiple players
/// - Pairs them when both are available
/// - Emits results for display
/// - Tracks rounds and terminates after completion
struct Game {
    ctx: Context<GuesserEvent>,
    player1_guess: Option<u8>,
    player2_guess: Option<u8>,
    round: u64,
}

impl Game {
    fn new(ctx: Context<GuesserEvent>) -> Self {
        Self {
            ctx,
            player1_guess: None,
            player2_guess: None,
            round: 0,
        }
    }
}

impl Actor for Game {
    type Event = GuesserEvent;

    async fn on_start(&mut self) -> maiko::Result<()> {
        // Send welcome message on startup
        self.ctx
            .send(GuesserEvent::Message(
                "Welcome to the Guessing Game!\n(the game will stop after 10 rounds)".to_string(),
            ))
            .await
    }

    /// Collect guesses from players and emit results when both have guessed.
    async fn handle_event(&mut self, envelope: &Envelope<Self::Event>) -> maiko::Result<()> {
        if let GuesserEvent::Guess { player, number } = envelope.event() {
            // Store the guess based on player ID
            match player {
                PlayerId::Player1 => self.player1_guess = Some(*number),
                PlayerId::Player2 => self.player2_guess = Some(*number),
            }

            // When both players have guessed, pair them and emit result
            if let (Some(n1), Some(n2)) = (self.player1_guess, self.player2_guess) {
                self.round += 1;

                // Clear guesses for next round
                self.player1_guess = None;
                self.player2_guess = None;

                // Emit result with correlation to track related events
                self.ctx
                    .send_child_event(
                        GuesserEvent::Result {
                            player1: n1,
                            player2: n2,
                        },
                        envelope.id(),
                    )
                    .await?;
            }
        }

        Ok(())
    }

    /// Check if the game should end and trigger shutdown.
    /// `ctx.stop_runtime()` initiates shutdown of the entire runtime.
    async fn step(&mut self) -> maiko::Result<StepAction> {
        if self.round >= 10 {
            self.ctx.stop_runtime()?;
        }
        Ok(StepAction::AwaitEvent)
    }
}

/// Output actor that formats and displays game events.
///
/// This is a **pure consumer** actor - it only reacts to events without emitting
/// new ones. It demonstrates the observer pattern where actors can watch events
/// without participating in the main logic.
struct Printer;

impl Actor for Printer {
    type Event = GuesserEvent;

    /// Display messages and results to the console.
    async fn handle_event(&mut self, envelope: &Envelope<Self::Event>) -> maiko::Result<()> {
        match envelope.event() {
            GuesserEvent::Message(msg) => {
                println!("{}", msg);
            }
            GuesserEvent::Result { player1, player2 } if player1 == player2 => {
                println!("Match! Both players guessed {}", player1);
            }
            GuesserEvent::Result { player1, player2 } => {
                println!("No match. Player1: {}, Player2: {}", player1, player2);
            }
            _ => {}
        }

        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut supervisor = Supervisor::<GuesserEvent, GuesserTopic>::default();

    // Add Player actors with no subscriptions - they only produce events
    supervisor.add_actor(
        "Player1",
        |ctx| Guesser::new(ctx, PlayerId::Player1, 500),
        Subscribe::none(),
    )?;
    supervisor.add_actor(
        "Player2",
        |ctx| Guesser::new(ctx, PlayerId::Player2, 350),
        Subscribe::none(),
    )?;

    // Game coordinator subscribes to Game topic (receives guesses)
    supervisor.add_actor("Game", Game::new, [GuesserTopic::Game])?;

    // Printer subscribes to Output topic (receives results and messages)
    supervisor.add_actor("Printer", |_| Printer, [GuesserTopic::Output])?;

    // Run until Game actor calls ctx.stop_runtime()
    supervisor.run().await?;

    println!("\nGame over!");
    Ok(())
}
