use std::collections::HashSet;

use crate::{
    world::sync::{auth_channel::AuthChannel, ordered_ids::OrderedIds},
    ComponentKind, EntityCommand, EntityMessage, EntityMessageType, HostEntity, HostType,
    MessageIndex,
};

/// Outbound state machine for a single host-owned entity, tracking its component set and authority sub-channel.
pub struct HostEntityChannel {
    component_channels: HashSet<ComponentKind>,
    auth_channel: AuthChannel,

    buffered_messages: OrderedIds<EntityMessage<()>>,
    incoming_messages: Vec<EntityMessage<()>>,
    outgoing_commands: Vec<EntityCommand>,

    /// Reserved auth-channel command to be emitted as `subcommand_id=0`
    /// the next time any auth-channel command is enqueued OR the
    /// outgoing buffer is drained. See [`reserve_first_command`] for
    /// the rationale (delegation's `MigrateResponse`-first invariant
    /// expressed explicitly at the channel level rather than relying
    /// on the synchronous call order in `enable_delegation_client_owned_entity`).
    reserved_first_command: Option<EntityCommand>,
}

impl HostEntityChannel {
    /// Creates a fresh `HostEntityChannel` with no components and default auth state for `host_type`.
    pub fn new(host_type: HostType) -> Self {
        Self {
            component_channels: HashSet::new(),
            auth_channel: AuthChannel::new(host_type),

            buffered_messages: OrderedIds::new(),
            incoming_messages: Vec::new(),
            outgoing_commands: Vec::new(),

            reserved_first_command: None,
        }
    }

    pub(crate) fn component_kinds(&self) -> &HashSet<ComponentKind> {
        &self.component_channels
    }

    /// Validates and routes `command` to the component set or authority sub-channel, queuing it for outbound delivery.
    pub fn send_command(&mut self, command: EntityCommand) {
        // Flush any reserved auth-channel command first so it lands at
        // subcommand_id=0 ahead of `command`. No-op if none reserved.
        self.flush_reserved_into_auth_channel();
        match command.get_type() {
            EntityMessageType::Spawn
            | EntityMessageType::SpawnWithComponents
            | EntityMessageType::Despawn
            | EntityMessageType::Noop => {
                panic!("These should be handled by the Engine, not the EntityChannelSender");
            }
            EntityMessageType::InsertComponent => {
                let component_kind = command.component_kind().unwrap();
                if self.component_channels.contains(&component_kind) {
                    panic!("Cannot insert a component that already exists in the entity channel");
                }
                self.component_channels.insert(component_kind);
                self.outgoing_commands.push(command);
            }
            EntityMessageType::RemoveComponent => {
                let component_kind = command.component_kind().unwrap();
                if !self.component_channels.contains(&component_kind) {
                    panic!("Cannot remove a component that does not exist in the entity channel");
                }
                self.component_channels.remove(&component_kind);
                self.outgoing_commands.push(command);
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
        // Ensure any reserved-first command is emitted even when no
        // other command follows it within this tick.
        self.flush_reserved_into_auth_channel();
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
            reserved_first_command: None,
        }
    }

    /// Drains and returns all queued outbound [`EntityCommand`]s.
    pub fn extract_outgoing_commands(&mut self) -> Vec<EntityCommand> {
        // Ensure any reserved-first command is emitted even when no
        // other command follows it within this tick.
        self.flush_reserved_into_auth_channel();
        std::mem::take(&mut self.outgoing_commands)
    }

    /// Reserve an auth-channel command to be emitted as the FIRST
    /// outbound command on this channel (`subcommand_id=0`).
    ///
    /// Background: the server's delegation path
    /// (`enable_delegation_client_owned_entity`) requires that
    /// `MigrateResponse` is the FIRST command in the new
    /// `HostEntityChannel` sequence so the client can sync its
    /// `next_subcommand_id=1` (see `RemoteEntityChannel`
    /// post-migration setup). Historically this invariant was
    /// implicit: the synchronous server code happened to call
    /// `host_send_migrate_response` first. Encoding it explicitly at
    /// the channel level decouples the invariant from caller order
    /// and unblocks deferring the Send-side delegation work to a
    /// later preamble drain (see MISSION_USER_ONLY_SEES_SIM Phase
    /// D.2 blocker 2).
    ///
    /// The reserved command will be enqueued (and consume
    /// `subcommand_id=0`) on the next of:
    ///   - any [`send_command`] call (the reserved command goes ahead
    ///     of the new command, which then takes `subcommand_id=1`),
    ///   - any drain via [`extract_outgoing_commands`] or
    ///     `drain_outgoing_messages_into`.
    ///
    /// Constraints:
    /// - Panics if a first-command is already reserved.
    /// - Panics if any auth-channel command has already been sent on
    ///   this channel (i.e. the would-be `subcommand_id=0` slot is
    ///   already gone).
    /// - The reserved command MUST be an auth-channel command type
    ///   (Publish / Unpublish / EnableDelegation / DisableDelegation /
    ///   SetAuthority / RequestAuthority / ReleaseAuthority /
    ///   EnableDelegationResponse / MigrateResponse). Lifecycle
    ///   commands (Spawn/Despawn/Noop) and component commands are
    ///   rejected with a panic.
    pub fn reserve_first_command(&mut self, command: EntityCommand) {
        if self.reserved_first_command.is_some() {
            panic!(
                "HostEntityChannel::reserve_first_command called twice before drain (type={:?})",
                command.get_type()
            );
        }
        if self.auth_channel.sender_has_sent_any() {
            panic!(
                "HostEntityChannel::reserve_first_command called after a command was already \
                 emitted on this channel; subcommand_id=0 slot has already been consumed \
                 (incoming type={:?})",
                command.get_type()
            );
        }
        match command.get_type() {
            EntityMessageType::Publish
            | EntityMessageType::Unpublish
            | EntityMessageType::EnableDelegation
            | EntityMessageType::DisableDelegation
            | EntityMessageType::SetAuthority
            | EntityMessageType::RequestAuthority
            | EntityMessageType::ReleaseAuthority
            | EntityMessageType::EnableDelegationResponse
            | EntityMessageType::MigrateResponse => {}
            other => {
                panic!(
                    "HostEntityChannel::reserve_first_command only accepts auth-channel command \
                     types; got {:?}",
                    other
                );
            }
        }
        self.reserved_first_command = Some(command);
    }

    /// If a first-command is reserved, validate + route it through the
    /// auth channel so it consumes `subcommand_id=0`. Idempotent
    /// once drained.
    fn flush_reserved_into_auth_channel(&mut self) {
        if let Some(reserved) = self.reserved_first_command.take() {
            self.auth_channel.validate_command(&reserved);
            self.auth_channel.send_command(reserved);
            self.auth_channel
                .sender_drain_messages_into(&mut self.outgoing_commands);
        }
    }

    /// Force-enable delegation on this channel (client-side only)
    /// This is called when the client originates an EnableDelegation message,
    /// to ensure the local channel is in the correct state to receive MigrateResponse
    pub fn local_enable_delegation(&mut self) {
        // Must publish first before enabling delegation
        self.auth_channel.force_publish();
        self.auth_channel.force_enable_delegation();
    }

    /// Returns `true` if this channel's authority sub-channel is in the Delegated state.
    pub fn is_delegated(&self) -> bool {
        self.auth_channel.is_delegated()
    }

    /// Returns the current publication/delegation state of this channel's authority sub-channel.
    pub fn auth_channel_state(&self) -> crate::world::sync::auth_channel::EntityAuthChannelState {
        self.auth_channel.state()
    }
}
