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

/// Publication/delegation lifecycle state of an entity's authority channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityAuthChannelState {
    /// Entity has not yet been published (client default).
    Unpublished,
    /// Entity is published and visible to other users but not yet delegated.
    Published,
    /// Authority delegation is active for this entity.
    Delegated,
}

pub(crate) struct AuthChannel {
    host_type: HostType,
    /// State this channel returns to on `reset()`. Differs between the host
    /// (send) and remote (receive) channels: see `new` vs `new_remote`.
    initial_state: EntityAuthChannelState,
    state: EntityAuthChannelState,
    auth_status: Option<EntityAuthStatus>,
    sender: AuthChannelSender,
    receiver: AuthChannelReceiver,
}

impl AuthChannel {
    /// Channel for entities this host OWNS and sends commands about.
    pub(crate) fn new(host_type: HostType) -> Self {
        let state = match host_type {
            HostType::Client => EntityAuthChannelState::Unpublished,
            HostType::Server => EntityAuthChannelState::Published,
        };
        Self::with_initial_state(host_type, state)
    }

    /// Channel for entities the PEER owns and we only receive messages about.
    ///
    /// The initial state is the peer's, so the mapping is the mirror of
    /// [`Self::new`]: a server's remote channels track client-owned entities,
    /// which begin `Unpublished`, while a client's remote channels track
    /// server-owned entities, which are `Published` from the start.
    pub(crate) fn new_remote(host_type: HostType) -> Self {
        let state = match host_type {
            HostType::Client => EntityAuthChannelState::Published,
            HostType::Server => EntityAuthChannelState::Unpublished,
        };
        Self::with_initial_state(host_type, state)
    }

    fn with_initial_state(host_type: HostType, state: EntityAuthChannelState) -> Self {
        Self {
            host_type,
            initial_state: state,
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

                if !Self::auth_status_transition_is_legal(from_status, *next_status) {
                    panic!(
                        "Invalid authority transition from {:?} to {:?}",
                        from_status, next_status
                    );
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

    /// Returns `true` if the sender has ever assigned a `SubCommandId`
    /// (i.e. its `next_subcommand_id` has advanced past 0). Used by
    /// `HostEntityChannel::reserve_first_command` to enforce that the
    /// `subcommand_id=0` slot is still available.
    pub(crate) fn sender_has_sent_any(&self) -> bool {
        self.sender.has_sent_any()
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
        *self = Self::with_initial_state(self.host_type, self.initial_state);
    }

    pub(crate) fn receiver_drain_messages_into(
        &mut self,
        outgoing_messages: &mut Vec<EntityMessage<()>>,
    ) {
        let mut drained = Vec::new();
        self.receiver.drain_messages_into(&mut drained);
        for msg in drained {
            if self.receiver_validate(&msg) {
                outgoing_messages.push(msg);
            }
        }
    }

    /// Receive-side counterpart to [`Self::validate_command`], advancing this
    /// channel's state as the peer's auth messages arrive.
    ///
    /// Returns `false` for a transition that is illegal from the current state,
    /// in which case the message is dropped and the state is left untouched.
    ///
    /// This DROPS rather than panics, which is the whole reason it is a
    /// separate function from `validate_command`. On the send path an illegal
    /// transition is a local programmer error and panicking is right. Here the
    /// message came off the wire from a peer, so on a server it is attacker-
    /// controlled -- panicking would hand any client a remote kill switch.
    /// Dropping keeps a malformed or malicious peer from corrupting our view of
    /// authority, without taking the host down.
    fn receiver_validate(&mut self, msg: &EntityMessage<()>) -> bool {
        use EntityAuthChannelState::{Delegated, Published, Unpublished};

        match msg.get_type() {
            EntityMessageType::Publish => {
                if self.state != Unpublished {
                    return false;
                }
                self.state = Published;
            }
            EntityMessageType::Unpublish => {
                if self.state != Published {
                    return false;
                }
                self.state = Unpublished;
            }
            EntityMessageType::EnableDelegation => {
                if self.state != Published {
                    return false;
                }
                self.state = Delegated;
                self.auth_status = Some(EntityAuthStatus::Available);
            }
            EntityMessageType::DisableDelegation => {
                if self.state != Delegated {
                    return false;
                }
                self.state = Published;
            }
            EntityMessageType::ReleaseAuthority => {
                if self.state != Delegated {
                    return false;
                }
                self.auth_status = Some(EntityAuthStatus::Available);
            }
            EntityMessageType::SetAuthority => {
                if self.state != Delegated {
                    return false;
                }
                let EntityMessage::SetAuthority(_, _, next_status) = msg else {
                    return false;
                };
                // No auth_status yet means we never saw the EnableDelegation
                // that establishes one; treat as illegal rather than unwrap.
                let Some(from_status) = self.auth_status else {
                    return false;
                };
                if !Self::auth_status_transition_is_legal(from_status, *next_status) {
                    return false;
                }
                self.auth_status = Some(*next_status);
            }
            EntityMessageType::RequestAuthority | EntityMessageType::EnableDelegationResponse => {
                if self.state != Delegated {
                    return false;
                }
            }
            // MigrateResponse is the FIRST command on a migrated entity's new
            // channel (see `HostEntityChannel`'s MigrateResponse-first
            // invariant), so unlike every other auth message it legitimately
            // arrives BEFORE the channel is Delegated -- it is what establishes
            // delegation, mirroring `configure_as_delegated`. Requiring
            // Delegated here would reject the very message that grants it.
            //
            // It only ever travels server -> client, so a client is the only
            // host that may accept one; a server receiving MigrateResponse is
            // hearing it from a client that has no business sending it.
            EntityMessageType::MigrateResponse => {
                if self.host_type != HostType::Client {
                    return false;
                }
                self.state = Delegated;
                if self.auth_status.is_none() {
                    self.auth_status = Some(EntityAuthStatus::Available);
                }
            }
            // Not an auth message; this channel does not gate it.
            _ => {}
        }
        true
    }

    /// The legal `EntityAuthStatus` edges, shared by the send and receive
    /// paths so the two cannot drift apart.
    fn auth_status_transition_is_legal(
        from_status: EntityAuthStatus,
        to_status: EntityAuthStatus,
    ) -> bool {
        matches!(
            (from_status, to_status),
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
                | (EntityAuthStatus::Releasing, EntityAuthStatus::Denied)
                // Same-state edges are idempotent no-ops by design; see the
                // duplicate-delivery note in `validate_command`.
                | (EntityAuthStatus::Available, EntityAuthStatus::Available)
                | (EntityAuthStatus::Requested, EntityAuthStatus::Requested)
                | (EntityAuthStatus::Granted, EntityAuthStatus::Granted)
                | (EntityAuthStatus::Denied, EntityAuthStatus::Denied)
                | (EntityAuthStatus::Releasing, EntityAuthStatus::Releasing)
        )
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
