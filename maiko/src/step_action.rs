use std::{fmt, hash, time::Duration};

/// Action returned by an actor `step` to influence scheduling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, hash::Hash, Default)]
pub enum StepAction {
    /// Run `step` again immediately.
    Continue,
    /// Yield to the runtime, then run `step` again.
    Yield,
    /// Pause `step` until the next event arrives.
    AwaitEvent,
    /// Sleep for the given duration before running `step` again.
    Backoff(Duration),
    /// Disable `step` permanently. This is the default.
    #[default]
    Never,
}

impl fmt::Display for StepAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StepAction::Continue => write!(f, "Continue"),
            StepAction::Yield => write!(f, "Yield"),
            StepAction::AwaitEvent => write!(f, "AwaitEvent"),
            StepAction::Backoff(_) => write!(f, "Backoff"),
            StepAction::Never => write!(f, "Never"),
        }
    }
}
