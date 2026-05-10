use std::collections::HashMap;

use log::warn;

use crate::{
    world::sync::{config::EngineConfig, HostEntityChannel},
    EntityCommand, EntityMessage, EntityMessageType, HostEntity, HostType,
    LocalEntityAndGlobalEntityConverter, MessageIndex,
};

pub struct HostEngine {
    host_type: HostType,
    pub config: EngineConfig,
    entity_channels: HashMap<HostEntity, HostEntityChannel>,

    incoming_events: Vec<EntityMessage<HostEntity>>,
    outgoing_commands: Vec<EntityCommand>,
}

impl HostEngine {
    pub(crate) fn new(host_type: HostType) -> Self {
        Self {
            host_type,
            config: EngineConfig::default(),
            entity_channels: HashMap::new(),

            incoming_events: Vec::new(),
            outgoing_commands: Vec::new(),
        }
    }

    pub(crate) fn take_incoming_events(&mut self) -> Vec<EntityMessage<HostEntity>> {
        std::mem::take(&mut self.incoming_events)
    }

    pub(crate) fn take_outgoing_commands(&mut self) -> Vec<EntityCommand> {
        std::mem::take(&mut self.outgoing_commands)
    }

    pub(crate) fn get_world(&self) -> &HashMap<HostEntity, HostEntityChannel> {
        &self.entity_channels
    }

    pub fn receive_message(&mut self, id: MessageIndex, msg: EntityMessage<HostEntity>) {
        match msg.get_type() {
            EntityMessageType::Spawn
            | EntityMessageType::SpawnWithComponents
            | EntityMessageType::Despawn
            | EntityMessageType::InsertComponent
            | EntityMessageType::RemoveComponent => {
                panic!(
                    "Host should not receive messages of this type: {:?}",
                    msg.get_type()
                );
            }
            EntityMessageType::Noop => {
                return;
            }
            _ => {}
        }

        let host_entity = msg.entity().unwrap();

        let Some(entity_channel) = self.entity_channels.get_mut(&host_entity) else {
            // Discard messages for unknown entities — this can happen with reordered or stale
            // packets from a buggy/lagging client after the entity has been despawned.
            warn!("host_engine: message for unknown entity {:?}, discarding", host_entity);
            return;
        };

        entity_channel.receive_message(id, msg.strip_entity());
        entity_channel.drain_incoming_messages_into(host_entity, &mut self.incoming_events);
    }

    /// Main entry point - validates command and returns it if valid
    /// This mirrors ReceiverEngine.accept_message() but for outgoing commands
    pub(crate) fn send_command(
        &mut self,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
        command: EntityCommand,
    ) {
        let global_entity = command.entity();
        let host_entity = converter
            .global_entity_to_host_entity(&global_entity)
            .unwrap();

        // info!("HostEngine::send_command(global entity={:?}, host_entity={:?}, command={:?})", global_entity, host_entity, command.get_type());

        match command.get_type() {
            EntityMessageType::Spawn => {
                if self.entity_channels.contains_key(&host_entity) {
                    panic!("Cannot spawn an entity that already exists in the engine");
                }
                self.entity_channels
                    .insert(host_entity, HostEntityChannel::new(self.host_type));
                self.outgoing_commands.push(command);
                return;
            }
            EntityMessageType::SpawnWithComponents => {
                if self.entity_channels.contains_key(&host_entity) {
                    panic!("Cannot spawn an entity that already exists in the engine");
                }
                let component_kinds = match &command {
                    EntityCommand::SpawnWithComponents(_, kinds) => {
                        kinds.iter().cloned().collect::<std::collections::HashSet<_>>()
                    }
                    _ => unreachable!(),
                };
                self.entity_channels.insert(
                    host_entity,
                    HostEntityChannel::new_with_components(self.host_type, component_kinds),
                );
                self.outgoing_commands.push(command);
                return;
            }
            EntityMessageType::Despawn => {
                if !self.entity_channels.contains_key(&host_entity) {
                    panic!("Cannot despawn an entity that does not exist in the engine");
                }
                // Remove the entity channel
                self.entity_channels.remove(&host_entity).unwrap();
                self.outgoing_commands.push(command);
                return;
            }
            EntityMessageType::Noop => {
                return;
            }
            _ => {}
        }

        let Some(entity_channel) = self.entity_channels.get_mut(&host_entity) else {
            panic!("Cannot accept command for an entity that does not exist in the engine. Command: {:?}", command);
        };

        entity_channel.send_command(command);
        entity_channel.drain_outgoing_messages_into(&mut self.outgoing_commands);
    }

    pub(crate) fn remove_entity_channel(&mut self, entity: &HostEntity) -> HostEntityChannel {
        self.entity_channels
            .remove(entity)
            .expect("Cannot remove entity channel that doesn't exist")
    }

    pub(crate) fn extract_entity_commands(&mut self, entity: &HostEntity) -> Vec<EntityCommand> {
        if let Some(channel) = self.entity_channels.get_mut(entity) {
            channel.extract_outgoing_commands()
        } else {
            Vec::new()
        }
    }

    pub(crate) fn insert_entity_channel(&mut self, entity: HostEntity, channel: HostEntityChannel) {
        if self.entity_channels.contains_key(&entity) {
            panic!("Cannot insert entity channel that already exists");
        }
        self.entity_channels.insert(entity, channel);
    }

    pub(crate) fn get_entity_channel(&self, entity: &HostEntity) -> Option<&HostEntityChannel> {
        self.entity_channels.get(entity)
    }

    pub(crate) fn get_entity_channel_mut(
        &mut self,
        entity: &HostEntity,
    ) -> Option<&mut HostEntityChannel> {
        self.entity_channels.get_mut(entity)
    }
}
