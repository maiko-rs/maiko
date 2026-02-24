use maiko::{monitoring::Monitor, *};

#[allow(unused)]
#[derive(Event, Clone, Debug)]
enum MyEvent {
    Hello(String),
}

// Create an actor
struct Greeter;

impl Actor for Greeter {
    type Event = MyEvent;

    async fn handle_event(&mut self, _envelope: &Envelope<Self::Event>) -> Result<()> {
        // Intentionally return an error to demonstrate on_error monitoring
        Ok(())
    }

    fn on_error(&self, _error: Error) -> Result<()> {
        // Swallow the error so the actor continues running
        Ok(())
    }
}

// Custom monitor demonstrating the Monitor trait.
// For simple tracing, consider using `maiko::monitors::Tracer` instead.
struct Printer;

impl Monitor<MyEvent> for Printer {
    fn on_event_handled(
        &self,
        envelope: &Envelope<MyEvent>,
        _topic: &DefaultTopic,
        actor_id: &ActorId,
    ) {
        println!("Event {:?} handled by actor {}", envelope.event(), actor_id);
    }

    fn on_error(&self, err: &str, actor_id: &ActorId) {
        eprintln!("Error in actor {}: {}", actor_id, err);
    }

    fn on_actor_stop(&self, actor_id: &ActorId) {
        println!("Actor {} has stopped", actor_id);
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut sup = Supervisor::<MyEvent>::default();

    // Add actor and subscribe it to all topics (DefaultTopic)
    sup.add_actor("greeter", |_ctx| Greeter, &[DefaultTopic])?;
    sup.monitors().add(Printer).await;

    // Start the supervisor and send a message
    sup.start().await?;
    sup.send(MyEvent::Hello("World".into())).await?;

    // Graceful shutdown (it attempts to process all events already in the queue)
    sup.stop().await?;
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    Ok(())
}
