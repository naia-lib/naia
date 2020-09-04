use std::{collections::VecDeque, rc::Rc};

use crate::naia_client::LocalEntityKey;
use naia_shared::{Event, EventType, SequenceBuffer};

const COMMAND_HISTORY_SIZE: u16 = 64;

/// Handles incoming, local, predicted Commands
#[derive(Debug)]
pub struct CommandReceiver<T: EventType> {
    queued_incoming_commands: VecDeque<(LocalEntityKey, Rc<Box<dyn Event<T>>>)>,
    command_history: SequenceBuffer<VecDeque<Rc<Box<dyn Event<T>>>>>,
}

impl<T: EventType> CommandReceiver<T> {
    /// Creates a new CommandSender
    pub fn new() -> Self {
        CommandReceiver {
            queued_incoming_commands: VecDeque::new(),
            command_history: SequenceBuffer::with_capacity(COMMAND_HISTORY_SIZE),
        }
    }

    /// Gets the next queued Command
    pub fn pop_command(&mut self) -> Option<(LocalEntityKey, Rc<Box<dyn Event<T>>>)> {
        self.queued_incoming_commands.pop_front()
    }

    /// Queues an Command to be ran locally on the Client
    pub fn queue_command(
        &mut self,
        host_tick: u16,
        local_entity_key: LocalEntityKey,
        command: &Rc<Box<dyn Event<T>>>,
    ) {
        self.queued_incoming_commands
            .push_front((local_entity_key, command.clone()));

        if !self.command_history.exists(host_tick) {
            self.command_history.insert(host_tick, VecDeque::new());
        }
        if let Some(queue) = self.command_history.get_mut(host_tick) {
            queue.push_back(command.clone());
        }
    }
}
