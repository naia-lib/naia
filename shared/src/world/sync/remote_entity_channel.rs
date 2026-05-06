//! ## `EntityChannel` – Per‑Entity Demultiplexer
//!
//! This module owns the **state machine and buffering logic for a *single
//! entity*** travelling across an **unordered, reliable** transport.
//!
//! ---
//! ### 1 · What problem does it solve?
//! * Messages can arrive *out of order*
//! * Certain message kinds must obey **strict causal order _within_ the
//!   entity** (e.g. a component can’t be inserted before the entity exists).
//!
//! `EntityChannel` absorbs the raw `EntityMessage<()>` stream, re‑orders and
//! filters it, and emits **ready‑to‑apply** messages in the *only* sequence
//! the game‑logic needs to respect.
//!
//! ---
//! ### 2 · State machine
//!
//! ```text
//!                 +-----------------------------+
//!                 |   Despawned (initial)       |
//!                 +-----------------------------+
//!                     | SpawnEntity(idₛ)  ▲
//!                     v                   |
//!                 +-----------------------------+
//!                 |     Spawned                 |
//!                 +-----------------------------+
//!                     | DespawnEntity(id_d)     |
//!                     +-------------------------+
//! ```
//!
//! * **`Despawned`** – entity is not present; buffers *only* the next
//!   `SpawnEntity` plus any later auth/component messages (they will flush
//!   once the spawn occurs).
//! * **`Spawned`** – entity is live; forwards component/auth messages to the
//!   corresponding sub‑channels and drains their output immediately.
//!
//! ---
//! ### 3 · Message ingest algorithm
//! 1. **Gating by `last_epoch_id `**
//!    A message whose `id ≤ last_epoch_id ` is *by definition* older than the
//!    authoritative `SpawnEntity`; drop it to guarantee *at‑most‑once
//!    semantics*; wrap‑around itself is handled automatically by the
//!    wrap‑safe `u16` comparison helpers—no epoch reset is performed.
//! 2. **Buffered queue (`OrderedIds`)**  
//!    Messages are pushed into `buffered_messages`, ordered by the `u16`
//!    sequence with wrap‑safe comparison.  
//!    `process_messages()` iterates from the head while the next candidate is
//!    *legal* under the current FSM state.
//! 3. **Draining**  
//!    Once a message is applied, it is moved into `outgoing_messages`.  
//!    `Engine::drain_messages_into` later annotates them with the concrete
//!    entity handle and forwards them to the ECS.
//!
//! ---
//! ### 4 · Sub‑channels
//! * **`AuthChannel`** – publishes, unpublishes, and delegates authority.
//! * **`ComponentChannel`** (one per `ComponentKind`) – tracks insert/remove
//!   toggles, guaranteeing idempotency via its own `last_insert_id` guard.
//!
//! `EntityChannel` coordinates these sub‑channels but *never* peers inside
//! their logic; it merely aligns their buffers with the entity’s lifecycle
//! (e.g., flush everything ≤ `idₛ` at spawn, reset on despawn).
//!
//! ---
//! ### 5 · Key invariants
//! * **Spawn barrier** – No component/auth message can overtake the spawn
//!   that legitimises it.
//! * **Monotonic visibility** – Once a message has been emitted to
//!   `outgoing_messages`, the channel guarantees it will never retract or
//!   reorder that message.
//!
//! Together, these guarantees let higher layers treat the engine as if every
//! entity had its own perfect *ordered* stream—while the network enjoys the
//! performance of a single unordered reliable channel.

use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
};

use crate::{
    sequence_less_than, world::sync::remote_component_channel::RemoteComponentChannel,
    ComponentKind, EntityAuthStatus, EntityCommand, EntityMessage, EntityMessageType, HostType,
    MessageIndex,
};

cfg_if! {
    if #[cfg(feature = "e2e_debug")] {
        use crate::world::host::host_world_manager::SubCommandId;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "e2e_debug", allow(dead_code))]
pub enum EntityChannelState {
    Despawned,
    Spawned,
}

pub struct RemoteEntityChannel {
    state: EntityChannelState,
    last_epoch_id: Option<MessageIndex>,

    component_channels: HashMap<ComponentKind, RemoteComponentChannel>,
    auth_channel: AuthChannel,

    buffered_messages: OrderedIds<EntityMessage<()>>,
    incoming_messages: Vec<EntityMessage<()>>,
    outgoing_commands: Vec<EntityCommand>,
}

impl RemoteEntityChannel {
    pub fn new(host_type: HostType) -> Self {
        Self {
            state: EntityChannelState::Despawned,
            last_epoch_id: None,

            component_channels: HashMap::new(),
            auth_channel: AuthChannel::new(host_type),

            buffered_messages: OrderedIds::new(),
            incoming_messages: Vec::new(),
            outgoing_commands: Vec::new(),
        }
    }

    /// Create a RemoteEntityChannel for a delegated entity (used during migration)
    ///
    /// After migration, MigrateResponse has subcommand_id=0, so the next message (SetAuthority)
    /// will have subcommand_id=1. We need to sync the receiver's next_subcommand_id accordingly.
    pub fn new_delegated(host_type: HostType) -> Self {
        let mut channel = Self::new(host_type);
        channel.configure_as_delegated();
        channel
    }

    pub fn configure_as_delegated(&mut self) {
        // Set up the AuthChannel for a delegated entity
        // This simulates the entity having gone through Publish → EnableDelegation
        self.auth_channel.force_publish();
        self.auth_channel.force_enable_delegation();
        // Sync subcommand_id: MigrateResponse has subcommand_id=0, so next is 1
        self.auth_channel.receiver_set_next_subcommand_id(1);
    }

    /// Update the AuthChannel's authority status (used after migration to sync with global status)
    pub fn update_auth_status(&mut self, auth_status: EntityAuthStatus) {
        self.auth_channel.force_set_auth_status(auth_status);
    }

    /// Get current auth status from internal AuthChannel (for testing)
    pub fn auth_status(&self) -> Option<EntityAuthStatus> {
        self.auth_channel.auth_status()
    }

    /// Check if AuthChannel is in delegated state (for testing)
    pub fn is_delegated(&self) -> bool {
        self.auth_channel.is_delegated()
    }

    pub(crate) fn receive_message(&mut self, id: MessageIndex, msg: EntityMessage<()>) {
        if let Some(last_epoch_id) = self.last_epoch_id {
            if last_epoch_id == id {
                panic!("EntityChannel received a message with the same id as the last epoch id. This should not happen. Message: {:?}", msg);
            }

            if sequence_less_than(id, last_epoch_id) {
                // This message is older than the last spawn message, ignore it
                return;
            }
        }

        self.buffered_messages.push_back(id, msg);

        self.process_messages();
    }

    pub fn send_command(&mut self, command: EntityCommand) {
        self.auth_channel.send_command(command);
        self.auth_channel
            .sender_drain_messages_into(&mut self.outgoing_commands);
    }

    pub(crate) fn drain_incoming_messages_into<E: Copy + Hash + Eq>(
        &mut self,
        entity: E,
        outgoing_events: &mut Vec<EntityMessage<E>>,
    ) {
        // Drain the entity channel and append the messages to the outgoing events
        let mut received_messages = Vec::new();
        for rmsg in std::mem::take(&mut self.incoming_messages) {
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

    #[allow(dead_code)]
    pub(crate) fn has_component_kind(&self, component_kind: &ComponentKind) -> bool {
        self.component_channels.contains_key(component_kind)
    }

    fn process_messages(&mut self) {
        loop {
            let Some((id, msg)) = self.buffered_messages.peek_front() else {
                break;
            };
            let id = *id;

            match msg.get_type() {
                EntityMessageType::Spawn => {
                    if self.state != EntityChannelState::Despawned {
                        break;
                    }

                    self.state = EntityChannelState::Spawned;
                    // Count when Spawn transitions state to Spawned
                    #[cfg(feature = "e2e_debug")]
                    {
                        extern "Rust" {
                            fn client_processed_spawn_increment();
                        }
                        unsafe {
                            client_processed_spawn_increment();
                        }
                    }
                    self.last_epoch_id = Some(id);
                    // clear buffered messages less than or equal to the last spawn id
                    self.buffered_messages.pop_front_until_and_excluding(id);

                    self.pop_front_into_outgoing();

                    // Drain the auth channel and append the messages to the outgoing events
                    self.auth_channel.receiver_buffer_pop_front_until_and_including(id);

                    self.auth_channel.receiver_process_messages(self.state);
                    self.auth_channel.receiver_drain_messages_into(&mut self.incoming_messages);

                    // Pop buffered messages from the component channels until and excluding the spawn id
                    // Then process the messages in the component channels
                    // Then drain the messages into the outgoing messages
                    for (component_kind, component_channel) in self.component_channels.iter_mut() {
                        component_channel.buffer_pop_front_until_and_excluding(id);
                        component_channel.process_messages(self.state);
                        component_channel.drain_messages_into(component_kind, &mut self.incoming_messages);
                    }
                }
                EntityMessageType::SpawnWithComponents => {
                    if self.state != EntityChannelState::Despawned {
                        break;
                    }

                    self.state = EntityChannelState::Spawned;
                    self.last_epoch_id = Some(id);
                    // Discard stale pre-lifetime messages buffered before this id
                    self.buffered_messages.pop_front_until_and_excluding(id);

                    // Pop the SpawnWithComponents message itself
                    let (_, msg) = self.buffered_messages.pop_front().unwrap();
                    let kinds = match msg {
                        EntityMessage::SpawnWithComponents((), kinds) => kinds,
                        _ => unreachable!(),
                    };

                    // Emit synthetic Spawn event
                    self.incoming_messages.push(EntityMessage::Spawn(()));

                    // Process any pre-buffered component channels (out-of-order arrivals)
                    for (component_kind, component_channel) in self.component_channels.iter_mut() {
                        component_channel.buffer_pop_front_until_and_excluding(id);
                        component_channel.process_messages(self.state);
                        component_channel.drain_messages_into(component_kind, &mut self.incoming_messages);
                    }

                    // Accept coalesced components: mark inserted + emit InsertComponent events
                    for kind in &kinds {
                        let component_channel = self.component_channels
                            .entry(*kind)
                            .or_insert_with(RemoteComponentChannel::new);
                        component_channel.set_inserted(true, id);
                        self.incoming_messages.push(EntityMessage::InsertComponent((), *kind));
                    }

                    // Drain auth channel
                    self.auth_channel.receiver_buffer_pop_front_until_and_including(id);
                    self.auth_channel.receiver_process_messages(self.state);
                    self.auth_channel.receiver_drain_messages_into(&mut self.incoming_messages);
                }
                EntityMessageType::Despawn => {
                    if self.state != EntityChannelState::Spawned {
                        break;
                    }

                    self.state = EntityChannelState::Despawned;
                    self.last_epoch_id = Some(id);

                    self.auth_channel.reset();
                    self.component_channels.clear();

                    self.pop_front_into_outgoing();

                    // clear the buffer
                    self.buffered_messages.clear();
                }
                EntityMessageType::InsertComponent | EntityMessageType::RemoveComponent => {

                    let (id, msg) = self.buffered_messages.pop_front().unwrap();
                    let component_kind = msg.component_kind().unwrap();
                    let component_channel = self.component_channels
                        .entry(component_kind)
                        .or_insert_with(RemoteComponentChannel::new);

                    component_channel.accept_message(self.state, id, msg);
                    component_channel.drain_messages_into(&component_kind, &mut self.incoming_messages);
                }
                EntityMessageType::Publish | EntityMessageType::Unpublish |
                EntityMessageType::EnableDelegation | EntityMessageType::DisableDelegation |
                EntityMessageType::ReleaseAuthority | // NOTE: This should be possible because a client might want to release authority right after enabling delegation
                EntityMessageType::SetAuthority => {
                    let (id, msg) = self.buffered_messages.pop_front().unwrap();
                    // info!("EntityChannelReceiver::process_messages(id={}, msgType={:?})", id, msg.get_type());

                    self.auth_channel.receiver_receive_message(Some(self.state), id, msg);
                    // Only drain auth messages when entity is Spawned (spawn barrier contract)
                    if self.state == EntityChannelState::Spawned {
                        self.auth_channel.receiver_drain_messages_into(&mut self.incoming_messages);
                    }
                    // When Despawned, message stays buffered in auth_channel until Spawn arm drains it
                }
                EntityMessageType::Noop => {
                    // Drop it
                }
                msg => {
                    panic!("EntityChannel::accept_message() received an unexpected message type: {:?}", msg);
                }
            }
        }
    }

    fn pop_front_into_outgoing(&mut self) {
        let (_, msg) = self.buffered_messages.pop_front().unwrap();
        self.incoming_messages.push(msg);
    }

    #[allow(dead_code)]
    pub(crate) fn get_state(&self) -> EntityChannelState {
        self.state
    }

    #[cfg(feature = "e2e_debug")]
    pub(crate) fn debug_auth_diagnostic(
        &self,
    ) -> (
        EntityChannelState,
        (SubCommandId, usize, Option<SubCommandId>, usize),
    ) {
        let auth_diag = self.auth_channel.receiver_debug_diagnostic();
        (self.state, auth_diag)
    }

    #[cfg(feature = "e2e_debug")]
    pub(crate) fn debug_channel_snapshot(
        &self,
    ) -> (
        EntityChannelState,
        Option<MessageIndex>,
        usize,
        Option<(MessageIndex, EntityMessageType)>,
        Option<MessageIndex>,
    ) {
        let state = self.state;
        let last_epoch_id = self.last_epoch_id;
        let buffered_len = self.buffered_messages.len();
        let head = self
            .buffered_messages
            .peek_front()
            .map(|(id, msg)| (*id, msg.get_type()));
        let spawn_id = self
            .buffered_messages
            .find_by_predicate(|msg| msg.get_type() == EntityMessageType::Spawn)
            .map(|(id, _)| id);
        (state, last_epoch_id, buffered_len, head, spawn_id)
    }

    pub(crate) fn extract_inserted_component_kinds(&self) -> HashSet<ComponentKind> {
        self.component_channels
            .iter()
            .filter(|(_, channel)| channel.is_inserted())
            .map(|(kind, _)| *kind)
            .collect()
    }

    pub(crate) fn force_drain_all_buffers(&mut self) {
        // Force-drain entity-level buffered messages
        while let Some((_, msg)) = self.buffered_messages.pop_front() {
            self.incoming_messages.push(msg);
        }

        // Force-drain all component channels
        for (_, component_channel) in self.component_channels.iter_mut() {
            component_channel.force_drain_buffers(self.state);
        }
    }

    pub(crate) fn insert_component(&mut self, component_kind: ComponentKind) {
        self.component_channels.entry(component_kind).or_insert_with(RemoteComponentChannel::new);
    }

    pub(crate) fn remove_component(&mut self, component_kind: ComponentKind) {
        self.component_channels.remove(&component_kind);
    }

    pub(crate) fn set_spawned(&mut self, epoch_id: MessageIndex) {
        if self.state != EntityChannelState::Despawned {
            panic!("Can only set spawned on despawned entity");
        }
        self.state = EntityChannelState::Spawned;
        self.last_epoch_id = Some(epoch_id);
    }

    pub(crate) fn insert_component_channel_as_inserted(
        &mut self,
        component_kind: ComponentKind,
        epoch_id: MessageIndex,
    ) {
        let mut comp_channel = RemoteComponentChannel::new();
        comp_channel.set_inserted(true, epoch_id);
        self.component_channels.insert(component_kind, comp_channel);
    }

    /// BULLETPROOF: Extract all incoming events for testing and validation
    #[allow(dead_code)]
    pub(crate) fn take_incoming_events(&mut self) -> Vec<EntityMessage<()>> {
        std::mem::take(&mut self.incoming_messages)
    }
}

use crate::world::sync::auth_channel::AuthChannel;
use crate::world::sync::ordered_ids::OrderedIds;

