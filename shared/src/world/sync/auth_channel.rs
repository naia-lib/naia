use crate::{
    world::{
        host::host_world_manager::SubCommandId,
        sync::{
            auth_channel_receiver::AuthChannelReceiver, auth_channel_sender::AuthChannelSender,
            remote_entity_channel::EntityChannelState,
        },
    },
    EntityAuthStatus, EntityCommand, EntityMessage, EntityMessageType, HostType, MessageIndex,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityAuthChannelState {
    Unpublished,
    Published,
    Delegated,
}

pub(crate) struct AuthChannel {
    host_type: HostType,
    state: EntityAuthChannelState,
    auth_status: Option<EntityAuthStatus>,
    sender: AuthChannelSender,
    receiver: AuthChannelReceiver,
}

impl AuthChannel {
    pub(crate) fn new(host_type: HostType) -> Self {
        let state = match host_type {
            HostType::Client => EntityAuthChannelState::Unpublished,
            HostType::Server => EntityAuthChannelState::Published,
        };
        Self {
            host_type,
            state,
            auth_status: None,
            sender: AuthChannelSender::new(),
            receiver: AuthChannelReceiver::new(),
        }
    }

    pub(crate) fn validate_command(&mut self, command: &EntityCommand) {
        let entity = command.entity();

        match command.get_type() {
            EntityMessageType::Publish => {
                if self.state != EntityAuthChannelState::Unpublished {
                    panic!(
                        "Cannot publish Entity: {:?} that is already published",
                        entity
                    );
                }
                self.state = EntityAuthChannelState::Published;
            }
            EntityMessageType::Unpublish => {
                if self.state != EntityAuthChannelState::Published {
                    panic!(
                        "Cannot unpublish Entity: {:?} that is not published",
                        entity
                    );
                }
                self.state = EntityAuthChannelState::Unpublished;
            }
            EntityMessageType::EnableDelegation => {
                if self.state != EntityAuthChannelState::Published {
                    panic!(
                        "Cannot enable delegation on Entity: {:?} that is not published",
                        entity
                    );
                }
                self.state = EntityAuthChannelState::Delegated;
                self.auth_status = Some(EntityAuthStatus::Available);
            }
            EntityMessageType::DisableDelegation => {
                #[cfg(feature = "e2e_debug")]
                crate::e2e_trace!(
                    "[CLIENT_RECV] DisableDelegation entity={:?} current_state={:?}",
                    entity,
                    self.state
                );
                if self.state != EntityAuthChannelState::Delegated {
                    panic!(
                        "Cannot disable delegation on Entity: {:?} that is not delegated",
                        entity
                    );
                }
                self.state = EntityAuthChannelState::Published;
            }
            EntityMessageType::ReleaseAuthority => {
                if self.state != EntityAuthChannelState::Delegated {
                    panic!(
                        "Cannot release authority on Entity: {:?} that is not delegated",
                        entity
                    );
                }

                // This is actually valid, because it should be possible for a client to ReleaseAuthority right after EnableDelegation, so that auth isn't automatically set to Granted
                self.auth_status = Some(EntityAuthStatus::Available);
            }
            EntityMessageType::SetAuthority => {
                if self.state != EntityAuthChannelState::Delegated {
                    panic!(
                        "Cannot set authority on Entity: {:?} that is not delegated",
                        entity
                    );
                }

                let EntityCommand::SetAuthority(_, _entity, next_status) = command else {
                    panic!("Expected SetAuthority command");
                };

                let from_status = self.auth_status.unwrap();
                #[cfg(feature = "e2e_debug")]
                crate::e2e_trace!(
                    "[CLIENT_RECV] SetAuthority entity={:?} from_status={:?} to_status={:?}",
                    command.entity(),
                    from_status,
                    next_status
                );

                match (from_status, next_status) {
                    (EntityAuthStatus::Available, EntityAuthStatus::Requested)
                    | (EntityAuthStatus::Available, EntityAuthStatus::Granted)
                    | (EntityAuthStatus::Available, EntityAuthStatus::Denied)
                    | (EntityAuthStatus::Requested, EntityAuthStatus::Granted)
                    | (EntityAuthStatus::Requested, EntityAuthStatus::Denied)
                    | (EntityAuthStatus::Requested, EntityAuthStatus::Available)
                    | (EntityAuthStatus::Denied, EntityAuthStatus::Granted)
                    | (EntityAuthStatus::Denied, EntityAuthStatus::Available)
                    | (EntityAuthStatus::Granted, EntityAuthStatus::Available)
                    | (EntityAuthStatus::Granted, EntityAuthStatus::Denied)
                    | (EntityAuthStatus::Granted, EntityAuthStatus::Releasing)
                    | (EntityAuthStatus::Releasing, EntityAuthStatus::Available)
                    | (EntityAuthStatus::Releasing, EntityAuthStatus::Denied) => {
                        // valid transition!
                    }
                    (from_status, to_status) => {
                        panic!(
                            "Invalid authority transition from {:?} to {:?}",
                            from_status, to_status
                        );
                    }
                }

                self.auth_status = Some(*next_status);
            }
            EntityMessageType::RequestAuthority => {
                // Client is requesting authority for a delegated entity
                if self.state != EntityAuthChannelState::Delegated {
                    panic!(
                        "Cannot request authority on Entity: {:?} that is not delegated",
                        entity
                    );
                }
                // Auth status will be updated by server's SetAuthority response
            }
            EntityMessageType::EnableDelegationResponse => {
                // Server is responding to delegation request
                // This is valid for entities that were just delegated
                if self.state != EntityAuthChannelState::Delegated {
                    panic!("Cannot send EnableDelegationResponse for Entity: {:?} that is not delegated", entity);
                }
            }
            EntityMessageType::MigrateResponse => {
                // Server is responding with entity migration information
                // This happens during delegation when entity ID changes
                // Valid for delegated entities
                if self.state != EntityAuthChannelState::Delegated {
                    panic!(
                        "Cannot send MigrateResponse for Entity: {:?} that is not delegated",
                        entity
                    );
                }
            }
            EntityMessageType::Noop => {
                // No-op command, always valid
            }
            e => {
                panic!("Unsupported command type for AuthChannelSender: {:?}", e);
            }
        }
    }

    pub(crate) fn send_command(&mut self, command: EntityCommand) {
        self.sender.send_command(command);
    }

    pub(crate) fn sender_drain_messages_into(&mut self, commands: &mut Vec<EntityCommand>) {
        self.sender.drain_messages_into(commands);
    }

    /// Get current state of the AuthChannel (for testing)
    pub fn state(&self) -> EntityAuthChannelState {
        self.state
    }

    /// Get current auth status (for testing)
    pub fn auth_status(&self) -> Option<EntityAuthStatus> {
        self.auth_status
    }

    /// Check if in delegated state (for testing)
    pub fn is_delegated(&self) -> bool {
        self.state == EntityAuthChannelState::Delegated
    }

    /// Is invoked by `EntityChannel` when the entity despawns; this wipes all buffered state so a future *re‑spawn* starts clean.
    pub(crate) fn reset(&mut self) {
        *self = Self::new(self.host_type);
    }

    pub(crate) fn receiver_drain_messages_into(
        &mut self,
        outgoing_messages: &mut Vec<EntityMessage<()>>,
    ) {
        self.receiver.drain_messages_into(outgoing_messages);
    }

    pub(crate) fn receiver_buffer_pop_front_until_and_including(&mut self, id: MessageIndex) {
        self.receiver.buffer_pop_front_until_and_including(id);
    }

    pub(crate) fn receiver_receive_message(
        &mut self,
        entity_state_opt: Option<EntityChannelState>,
        id: MessageIndex,
        msg: EntityMessage<()>,
    ) {
        self.receiver.receive_message(entity_state_opt, id, msg);
    }

    pub(crate) fn receiver_process_messages(&mut self, entity_state: EntityChannelState) {
        self.receiver.process_messages(Some(entity_state));
    }

    /// Set the next expected subcommand_id in the receiver (used after migration to sync with server's sequence)
    pub(crate) fn receiver_set_next_subcommand_id(&mut self, id: SubCommandId) {
        self.receiver.set_next_subcommand_id(id);
    }

    /// Force the AuthChannel into Published state (used during migration setup)
    pub(crate) fn force_publish(&mut self) {
        self.state = EntityAuthChannelState::Published;
    }

    /// Force the AuthChannel into Delegated state with Available authority (used during migration setup)
    pub(crate) fn force_enable_delegation(&mut self) {
        self.state = EntityAuthChannelState::Delegated;
        self.auth_status = Some(EntityAuthStatus::Available);
    }

    /// Force set the authority status (used to sync with global authority tracker after migration)
    pub(crate) fn force_set_auth_status(&mut self, auth_status: EntityAuthStatus) {
        self.auth_status = Some(auth_status);
    }

    #[cfg(feature = "e2e_debug")]
    pub(crate) fn receiver_debug_diagnostic(
        &self,
    ) -> (SubCommandId, usize, Option<SubCommandId>, usize) {
        self.receiver.debug_diagnostic()
    }
}
