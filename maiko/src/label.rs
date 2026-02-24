use std::borrow::Cow;

/// Human-readable label for an event variant.
///
/// Used by the test harness for event matching (e.g. `chain.events().contains("KeyPress")`),
/// Mermaid diagram generation, and the `Recorder` monitor for JSON output.
///
/// Derive it automatically with `#[derive(Label)]` (requires the `macros` feature).
///
/// # Example
///
/// ```rust
/// use maiko::Label;
///
/// #[derive(Label)]
/// enum MyTopic {
///     SensorData,
///     Alerts,
/// }
///
/// assert_eq!(MyTopic::SensorData.label(), "SensorData");
/// assert_eq!(MyTopic::Alerts.label(), "Alerts");
/// ```
pub trait Label {
    /// Returns a human-readable label for this item.
    fn label(&self) -> Cow<'static, str>;
}
