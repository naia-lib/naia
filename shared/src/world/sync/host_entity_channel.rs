use std::collections::HashSet;

use crate::{
    world::sync::{auth_channel::AuthChannel, ordered_ids::OrderedIds},
    ComponentKind, EntityCommand, EntityMessage, EntityMessageType, HostEntity, HostType,
    MessageIndex,
};

pub struct HostEntityChannel {
    component_channels: HashSet<ComponentKind>,
    auth_channel: AuthChannel,

    buffered_messages: OrderedIds<EntityMessage<()>>,
    incoming_messages: Vec<EntityMessage<()>>,
    outgoing_commands: Vec<EntityCommand>,
}

impl HostEntityChannel {
    pub fn new(host_type: HostType) -> Self {
        Self {
            component_channels: HashSet::new(),
            auth_channel: AuthChannel::new(host_type),

            buffered_messages: OrderedIds::new(),
            incoming_messages: Vec::new(),
            outgoing_commands: Vec::new(),
        }
    }

    pub(crate) fn component_kinds(&self) -> &HashSet<ComponentKind> {
        &self.component_channels
    }

    pub fn send_command(&mut self, command: EntityCommand) {
        match command.get_type() {
            EntityMessageType::Spawn | EntityMessageType::Despawn | EntityMessageType::Noop => {
                panic!("These should be handled by the Engine, not the EntityChannelSender");
            }
            EntityMessageType::InsertComponent => {
                let component_kind = command.component_kind().unwrap();
                if self.component_channels.contains(&component_kind) {
                    panic!("Cannot insert a component that already exists in the entity channel");
                }
                self.component_channels.insert(component_kind);
                self.outgoing_commands.push(command);
                return;
            }
            EntityMessageType::RemoveComponent => {
                let component_kind = command.component_kind().unwrap();
                if !self.component_channels.contains(&component_kind) {
                    panic!("Cannot remove a component that does not exist in the entity channel");
                }
                self.component_channels.remove(&component_kind);
                self.outgoing_commands.push(command);
                return;
            }
            EntityMessageType::Publish
            | EntityMessageType::Unpublish
            | EntityMessageType::EnableDelegation
            | EntityMessageType::DisableDelegation
            | EntityMessageType::SetAuthority
            | EntityMessageType::RequestAuthority
            | EntityMessageType::ReleaseAuthority
            | EntityMessageType::EnableDelegationResponse
            | EntityMessageType::MigrateResponse => {
                self.auth_channel.validate_command(&command);
                self.auth_channel.send_command(command);
                self.auth_channel
                    .sender_drain_messages_into(&mut self.outgoing_commands);
                return;
            }
        }
    }

    pub(crate) fn drain_incoming_messages_into(
        &mut self,
        entity: HostEntity,
        outgoing_events: &mut Vec<EntityMessage<HostEntity>>,
    ) {
        // Drain the entity channel and append the messages to the outgoing events
        let mut received_messages = Vec::new();
        for rmsg in std::mem::take(&mut self.incoming_messages) {
            // info!("EntityChannelSender::drain_incoming_messages_into(entity={:?}, msgType={:?})", entity, rmsg.get_type());

            received_messages.push(rmsg.with_entity(entity));
        }
        outgoing_events.append(&mut received_messages);
    }

    pub(crate) fn drain_outgoing_messages_into(
        &mut self,
        outgoing_commands: &mut Vec<EntityCommand>,
    ) {
        outgoing_commands.append(&mut self.outgoing_commands);
    }

    pub(crate) fn receive_message(&mut self, id: MessageIndex, msg: EntityMessage<()>) {
        self.buffered_messages.push_back(id, msg);

        self.process_messages();
    }

    fn process_messages(&mut self) {
        loop {
            let Some((_id, msg)) = self.buffered_messages.peek_front() else {
                break;
            };

            match msg.get_type() {
                EntityMessageType::RequestAuthority
                | EntityMessageType::ReleaseAuthority
                | EntityMessageType::EnableDelegationResponse
                | EntityMessageType::MigrateResponse => {
                    let (id, msg) = self.buffered_messages.pop_front().unwrap();

                    // info!("EntityChannelSender::process_messages(id={}, msgType={:?})", id, msg.get_type());

                    self.auth_channel.receiver_receive_message(None, id, msg);
                    self.auth_channel
                        .receiver_drain_messages_into(&mut self.incoming_messages);
                }
                EntityMessageType::Noop => {
                    // Drop it
                }
                msg => {
                    panic!("EntityChannelSender::process_messages() received an unexpected message type: {:?}", msg);
                }
            }
        }
    }

    pub(crate) fn new_with_components(
        host_type: HostType,
        component_kinds: HashSet<ComponentKind>,
    ) -> Self {
        Self {
            component_channels: component_kinds,
            auth_channel: AuthChannel::new(host_type),
            buffered_messages: OrderedIds::new(),
            incoming_messages: Vec::new(),
            outgoing_commands: Vec::new(),
        }
    }

    pub fn extract_outgoing_commands(&mut self) -> Vec<EntityCommand> {
        std::mem::take(&mut self.outgoing_commands)
    }

    /// Force-enable delegation on this channel (client-side only)
    /// This is called when the client originates an EnableDelegation message,
    /// to ensure the local channel is in the correct state to receive MigrateResponse
    pub fn local_enable_delegation(&mut self) {
        // Must publish first before enabling delegation
        self.auth_channel.force_publish();
        self.auth_channel.force_enable_delegation();
    }

    /// Check if this channel is in Delegated state (for testing)
    pub fn is_delegated(&self) -> bool {
        self.auth_channel.is_delegated()
    }

    /// Get the AuthChannel state (for testing)
    pub fn auth_channel_state(&self) -> crate::world::sync::auth_channel::EntityAuthChannelState {
        self.auth_channel.state()
    }
}
