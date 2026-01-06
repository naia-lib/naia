//! # `engine.rs` — Top‑Level Orchestrator
//!
//! The **`Engine<E>`** is the *single entry/exit point* between the raw,
//! unordered stream of `EntityMessage<E>` packets on the wire and the
//! **ordered, per‑entity event queue** your game logic consumes.
//! It owns *one* [`RemoteEntityChannel`] per live entity and two lightweight
//! collections for runtime bookkeeping:
//!
//! | Field | Purpose |
//! |-------|---------|
//! | `config`            | Compile‑time knobs from [`EngineConfig`] that bound the sliding window and guard‑band for wrap‑around safety. |
//! | `outgoing_events`   | Scratch buffer filled during `accept_message`; drained atomically via [`receive_messages`]. |
//! | `entity_channels`   | `HashMap<E, EntityChannel>` lazily populated on first sight of an entity. |
//!
//! ## Responsibilities
//! 1. **Channel dispatch** – routes each message to its entity’s channel,
//!    creating channels on demand.
//! 2. **Local ordering** – relies on per‑channel state machines to decide
//!    *when* a message is safe to surface; glues their outputs into a
//!    single, ready‑to‑apply Vec.
//! 3. **Zero HoLB guarantee** – because messages for unrelated entities
//!    never share the same queue, one delayed entity cannot stall others.
//!
//! ## API contracts
//!
//! ## Interaction with `EngineConfig`
//! The `Engine` never mutates sequence numbers, but it does rely on the
//! sender/receiver honouring `max_in_flight` and `flush_threshold` to
//! avoid ambiguous wrapping (`u16` rolls over every 65536).
//! *If you change these constants, do so symmetrically on both ends.*

use std::{collections::HashMap, fmt::Debug, hash::Hash};

use crate::EntityCommand;
use crate::{
    world::{
        entity::entity_message::EntityMessage,
        sync::{config::EngineConfig, remote_entity_channel::RemoteEntityChannel},
    },
    EntityAuthStatus, EntityMessageType, HostType, InScopeEntities, MessageIndex, RemoteEntity,
};

pub struct RemoteEngine<E: Copy + Hash + Eq + Debug> {
    host_type: HostType,
    pub config: EngineConfig,
    entity_channels: HashMap<E, RemoteEntityChannel>,

    incoming_events: Vec<EntityMessage<E>>,
    outgoing_commands: Vec<EntityCommand>,
}

impl<E: Copy + Hash + Eq + Debug> RemoteEngine<E> {
    pub(crate) fn new(host_type: HostType) -> Self {
        Self {
            host_type,
            config: EngineConfig::default(),
            incoming_events: Vec::new(),
            entity_channels: HashMap::new(),
            outgoing_commands: Vec::new(),
        }
    }

    /// Atomically swaps out `outgoing_events`, giving the caller a Vec that
    /// *is already topologically ordered across entities*; apply each event
    /// in sequence and discard.
    pub(crate) fn take_incoming_events(&mut self) -> Vec<EntityMessage<E>> {
        std::mem::take(&mut self.incoming_events)
    }

    pub(crate) fn take_outgoing_commands(&mut self) -> Vec<EntityCommand> {
        std::mem::take(&mut self.outgoing_commands)
    }

    pub(crate) fn get_world(&self) -> &HashMap<E, RemoteEntityChannel> {
        &self.entity_channels
    }

    /// * Idempotent*: the caller must already have deduplicated on
    /// `(MessageIndex, Entity)`; re‑injecting the same `(id, msg)` WILL panic!
    ///
    /// *Non‑blocking*: may push zero or more *ordered* events into the
    /// engine’s outgoing buffer, but never touches the ECS directly.
    pub fn receive_message(&mut self, id: MessageIndex, msg: EntityMessage<E>) {
        match msg.get_type() {
            EntityMessageType::Noop => {
                return;
            }
            _ => {}
        }

        let entity = msg.entity().unwrap();

        // If the entity channel does not exist, create it
        if !self.entity_channels.contains_key(&entity) {
            self.insert_entity_channel(entity, RemoteEntityChannel::new(self.host_type))
        }
        let entity_channel = self.entity_channels.get_mut(&entity).unwrap();

        // if log {
        //     info!("Engine::accept_message(id={}, entity={:?}, msgType={:?})", id, entity, msg.get_type());
        // }

        entity_channel.receive_message(id, msg.strip_entity());
        entity_channel.drain_incoming_messages_into(entity, &mut self.incoming_events);
    }

    ///
    pub fn send_auth_command(&mut self, entity: E, command: EntityCommand) {
        if !self.entity_channels.contains_key(&entity) {
            panic!(
                "Cannot send a command to an entity that does not exist in the engine: {:?}",
                entity
            );
        }

        let entity_channel = self.entity_channels.get_mut(&entity).unwrap();
        entity_channel.send_command(command);
        entity_channel.drain_outgoing_messages_into(&mut self.outgoing_commands);
    }

    /// Update authority status in RemoteEntityChannel's AuthChannel (used after migration)
    pub fn receive_set_auth_status(&mut self, entity: E, auth_status: EntityAuthStatus) {
        if let Some(channel) = self.entity_channels.get_mut(&entity) {
            channel.update_auth_status(auth_status);
        }
    }

    /// Get auth status of an entity's channel (for testing)
    pub fn get_entity_auth_status(&self, entity: &E) -> Option<EntityAuthStatus> {
        self.entity_channels
            .get(entity)
            .and_then(|channel| channel.auth_status())
    }

    pub fn send_entity_command(&mut self, entity: E, command: EntityCommand) {
        if !self.entity_channels.contains_key(&entity) {
            panic!(
                "Cannot send a command to an entity that does not exist in the engine: {:?}",
                entity
            );
        }

        // Handle entity commands for RemoteEngine
        match command {
            EntityCommand::Despawn(_) => {
                // Remove the entity channel
                self.entity_channels.remove(&entity);
            }
            EntityCommand::InsertComponent(_, component_kind) => {
                // Insert component into the entity channel
                if let Some(channel) = self.entity_channels.get_mut(&entity) {
                    channel.insert_component(component_kind);
                }
            }
            EntityCommand::RemoveComponent(_, component_kind) => {
                // Remove component from the entity channel
                if let Some(channel) = self.entity_channels.get_mut(&entity) {
                    channel.remove_component(component_kind);
                }
            }
            _ => {
                // Other commands are handled by the auth system or are not applicable
                // to RemoteEngine (like Publish, Unpublish, etc.)
            }
        }
    }

    pub(crate) fn remove_entity_channel(&mut self, entity: &E) -> RemoteEntityChannel {
        self.entity_channels
            .remove(entity)
            .expect("Cannot remove entity channel that doesn't exist")
    }

    pub(crate) fn insert_entity_channel(&mut self, entity: E, channel: RemoteEntityChannel) {
        if self.entity_channels.contains_key(&entity) {
            panic!(
                "Cannot insert entity channel that already exists for entity: {:?}",
                entity
            );
        }
        self.entity_channels.insert(entity, channel);
    }

    pub(crate) fn has_entity(&self, entity: &E) -> bool {
        self.entity_channels.contains_key(entity)
    }

    pub(crate) fn get_entity_channel_mut(
        &mut self,
        entity: &E,
    ) -> Option<&mut RemoteEntityChannel> {
        self.entity_channels.get_mut(entity)
    }

    pub(crate) fn get_world_mut(&mut self) -> &mut HashMap<E, RemoteEntityChannel> {
        &mut self.entity_channels
    }
}

impl InScopeEntities<RemoteEntity> for RemoteEngine<RemoteEntity> {
    fn has_entity(&self, entity: &RemoteEntity) -> bool {
        self.get_world().contains_key(entity)
    }
}
