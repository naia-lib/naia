use std::collections::HashMap;

use naia_shared::{LocalEntity, ProtocolType, Ref, Replicate};

/// Handles outgoing Commands
#[derive(Debug)]
pub struct CommandSender<P: ProtocolType> {
    queued_outgoing_command: HashMap<LocalEntity, Ref<dyn Replicate<P>>>,
}

impl<P: ProtocolType> CommandSender<P> {
    /// Creates a new CommandSender
    pub fn new() -> Self {
        CommandSender {
            queued_outgoing_command: HashMap::new(),
        }
    }

    /// Gets the next queued Command to be transmitted
    pub fn has_command(&self) -> bool {
        self.queued_outgoing_command.len() != 0
    }

    /// Gets the next queued Command to be transmitted
    pub fn pop_command(&mut self) -> Option<(LocalEntity, Ref<dyn Replicate<P>>)> {
        let mut out_key = None;
        if let Some((key, _)) = self.queued_outgoing_command.iter().next() {
            out_key = Some(*key);
        }

        if let Some(key) = out_key {
            if let Some(command) = self.queued_outgoing_command.remove(&key) {
                return Some((key, command));
            }
        }
        return None;
    }

    /// If  the last popped Command from the queue somehow wasn't able to be
    /// written into a packet, put the Command back into the front of the queue
    pub fn unpop_command(&mut self, prediction_key: &LocalEntity, command: &Ref<dyn Replicate<P>>) {
        let cloned_command = command.clone();
        self.queued_outgoing_command
            .insert(*prediction_key, cloned_command);
    }

    /// Queues an Command to be transmitted to the remote host
    pub fn queue_command(&mut self, prediction_key: &LocalEntity, command: &Ref<dyn Replicate<P>>) {
        self.queued_outgoing_command
            .insert(*prediction_key, command.clone());
    }
}
