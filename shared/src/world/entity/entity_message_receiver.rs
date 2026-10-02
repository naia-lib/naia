use std::{fmt::Debug, hash::Hash};

use crate::{
    messages::channels::receivers::reliable_receiver::ReliableReceiver,
    world::sync::{HostEngine, RemoteEngine},
    EntityMessage, HostEntity, MessageIndex, Tick,
};

/// Stateless helper that routes incoming entity messages into per-entity ordered queues.
pub struct EntityMessageReceiver;

impl EntityMessageReceiver {
    /// Buffer a read [`EntityMessage`] so that it can be processed later
    pub fn buffer_message<E: Copy + Hash + Eq + Debug>(
        receiver: &mut ReliableReceiver<EntityMessage<E>>,
        message_index: MessageIndex,
        message: EntityMessage<E>,
    ) {
        receiver.buffer_message(message_index, message);
    }

    /// Read all buffered [`EntityMessage`] inside the `receiver` and process them.
    ///
    /// Outputs the list of [`EntityMessage`] that can be executed now, buffer the rest
    /// into each entity's `EntityChannelReceiver`
    pub fn remote_take_incoming_messages<E: Copy + Hash + Eq + Debug>(
        remote_engine: &mut RemoteEngine<E>,
        incoming_messages: Vec<(MessageIndex, Tick, EntityMessage<E>)>,
    ) -> Vec<(Tick, EntityMessage<E>)> {
        for (message_index, tick, message) in incoming_messages {
            remote_engine.receive_message(message_index, tick, message);
        }
        remote_engine.take_incoming_events()
    }

    /// Feeds `incoming_messages` into `host_engine` and returns all events now ready to apply.
    // TODO: refactor this to use a generic type for the engine
    pub fn host_take_incoming_events(
        host_engine: &mut HostEngine,
        incoming_messages: Vec<(MessageIndex, Tick, EntityMessage<HostEntity>)>,
    ) -> Vec<(Tick, EntityMessage<HostEntity>)> {
        for (message_index, tick, message) in incoming_messages {
            host_engine.receive_message(message_index, tick, message);
        }
        host_engine.take_incoming_events()
    }
}
