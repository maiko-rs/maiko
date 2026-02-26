use tokio::sync::broadcast;

use super::Command;
use crate::{Error, Result};

#[repr(transparent)]
#[derive(Clone)]
pub struct CommandSender(broadcast::Sender<Command>);

impl CommandSender {
    pub fn send(&self, cmd: Command) -> Result {
        self.0.send(cmd).map_err(Error::internal)?;
        Ok(())
    }
}

impl From<broadcast::Sender<Command>> for CommandSender {
    fn from(sender: broadcast::Sender<Command>) -> Self {
        CommandSender(sender)
    }
}

impl AsRef<broadcast::Sender<Command>> for CommandSender {
    fn as_ref(&self) -> &broadcast::Sender<Command> {
        &self.0
    }
}
