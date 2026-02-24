/// Marker trait for events processed by Maiko.
///
/// Implement this for your event type (often an enum). Events must be
/// `Send + Sync + Clone + 'static` because they:
/// - Are wrapped in `Arc<`[`Envelope<E>`](crate::Envelope)`>` and shared across threads (Sync)
/// - Cross task boundaries and live in spawned tasks (Send, 'static)
/// - Are routed to multiple subscribers (Clone)
///
/// Use `#[derive(Event)]` instead of implementing this trait manually.
/// Consider also deriving [`Label`](crate::Label) for human-readable
/// variant names in diagnostics and test output.
///
/// # Example
///
/// ```rust
/// use maiko::Event;
///
/// #[derive(Clone, Debug, Event)]
/// enum ChatEvent {
///     Message(String),
///     Join(String),
/// }
/// ```
pub trait Event: Send + Sync + Clone + 'static {}
