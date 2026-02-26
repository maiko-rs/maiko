use crate::{ActorId, Envelope, Event, Topic, monitoring::Monitor};
use serde::Serialize;
use std::cell::RefCell;
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

/// A monitor that records events to a file in JSON Lines format.
///
/// Each dispatched event is written as a JSON object on its own line,
/// making the output easy to parse and stream. Events are flushed
/// immediately for reliability (not optimized for high-throughput).
///
/// # Example
///
/// ```ignore
/// let recorder = Recorder::new("events.jsonl")?;
/// sup.monitors().add(recorder).await;
/// ```
#[derive(Debug)]
pub struct Recorder {
    writer: RefCell<BufWriter<File>>,
}

// RefCell is Send (inner type is Send), and Monitor only requires Send, not Sync.
// Single-threaded dispatcher context makes interior mutability safe here.

impl Recorder {
    /// Create a new recorder that writes to the specified path.
    ///
    /// # Errors
    ///
    /// Returns [`std::io::Error`] if the file cannot be created.
    pub fn new<P: AsRef<Path>>(path: P) -> std::io::Result<Self> {
        let file = File::create(path)?;
        Ok(Self {
            writer: RefCell::new(BufWriter::new(file)),
        })
    }
}

impl<E, T> Monitor<E, T> for Recorder
where
    E: Event + Serialize,
    T: Topic<E>,
{
    fn on_event_dispatched(&self, envelope: &Envelope<E>, _topic: &T, _receiver: &ActorId) {
        if let Ok(mut writer) = self.writer.try_borrow_mut() {
            if let Err(e) = serde_json::to_writer(&mut *writer, envelope) {
                tracing::warn!("Recorder failed to serialize event: {}", e);
            }
            let _ = std::io::Write::write_all(&mut *writer, b"\n");
            let _ = std::io::Write::flush(&mut *writer);
        } else {
            tracing::warn!("Recorder failed to borrow writer");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DefaultTopic;
    use serde::Serialize;
    use std::io::Read;

    #[derive(Clone, Debug, Serialize)]
    struct TestEvent(String);
    impl Event for TestEvent {}

    #[test]
    fn test_recorder_writes_json() {
        let path = std::env::temp_dir().join("maiko_recorder_test.jsonl");
        let recorder = Recorder::new(&path).expect("Failed to create recorder");

        let event = TestEvent("hello".to_string());
        let sender_id = ActorId::new("sender");
        let envelope = Envelope::new(event.clone(), sender_id);
        let receiver_id = ActorId::new("receiver");

        recorder.on_event_dispatched(&envelope, &DefaultTopic, &receiver_id);

        let mut file = File::open(&path).expect("Failed to open log file");
        let mut content = String::new();
        file.read_to_string(&mut content)
            .expect("Failed to read log file");

        assert!(content.contains("hello"));
        assert!(content.contains("sender"));

        let _ = std::fs::remove_file(&path);
    }
}
