//! Authority & Delegation Channel  
//! ==============================
//!
//! Maintains the *authoritative‑owner* state for a single entity across an
//! unordered‑reliable transport.  `AuthChannel` is a **tiny state machine**
//! that filters, buffers, and eventually forwards only *causally‑legal*
//! authority messages to the outer `EntityChannel`.
//!
//! ## High‑level purpose
//! * Decouple global out‑of‑order arrival from the strict ordering
//!   requirements of authority negotiation.
//! * Guarantee that the ECS sees at most **one semantically valid sequence**
//!   of publish / delegate / authority‑update events, even if the network
//!   reorders packets.
//!
//! ## Accepted `EntityMessage` variants
//! | Variant                              | Meaning on receive | Requires state |
//! |--------------------------------------|--------------------|----------------|
//! | `PublishEntity`                      | Make entity visible to client | `Unpublished` |
//! | `UnpublishEntity`                    | Hide / delete entity          | `Published` |
//! | `EnableDelegationEntity`             | Allow authority hand‑offs     | `Published` |
//! | `DisableDelegationEntity`            | Revoke delegation             | `Delegated` |
//! | `EntityUpdateAuthority { … }`        | Inform who currently owns it  | `Delegated` |
//!
//! ## State machine
//! ```text
//!             +--------------------+
//!             |    Unpublished     |
//!             +---------+----------+
//!                       | PublishEntity
//!                       v
//!             +--------------------+
//!             |     Published      |
//!             +----+-----------+---+
//!                  |           |
//!  UnpublishEntity |           | EnableDelegationEntity
//!                  v           v
//!             +--------------------+
//!             |     Delegated      |
//!             +-----------+--------+
//!                         | DisableDelegationEntity
//!                         +-------------------------> back to *Published*
//! ```
//! `EntityUpdateAuthority` is a self‑loop in the **Delegated** state.
//!
//! **Invariant**: The channel never exports a message that would violate
//! the canonical state graph above; thus consumers can apply events in
//! arrival order without additional checks.

use crate::world::sync::ordered_ids::OrderedIds;
use crate::{
    world::{
        host::host_world_manager::SubCommandId, sync::remote_entity_channel::EntityChannelState,
    },
    EntityMessage, MessageIndex,
};

pub(crate) struct AuthChannelReceiver {
    next_subcommand_id: SubCommandId,
    buffered_messages: OrderedIds<EntityMessage<()>>,
    incoming_messages: Vec<EntityMessage<()>>,
}

impl AuthChannelReceiver {
    pub(crate) fn new() -> Self {
        Self {
            next_subcommand_id: 0,
            buffered_messages: OrderedIds::new(),
            incoming_messages: Vec::new(),
        }
    }

    /// Is invoked by `EntityChannel` when the entity despawns; this wipes all buffered state so a future *re‑spawn* starts clean.
    #[allow(dead_code)]
    pub(crate) fn reset(&mut self) {
        *self = Self::new();
    }

    #[allow(dead_code)]
    pub(crate) fn reset_next_subcommand_id(&mut self) {
        self.next_subcommand_id = 0;
    }

    /// Set the next expected subcommand_id (used after migration to sync with server's sequence)
    pub(crate) fn set_next_subcommand_id(&mut self, id: SubCommandId) {
        self.next_subcommand_id = id;
    }

    pub(crate) fn drain_messages_into(&mut self, outgoing_messages: &mut Vec<EntityMessage<()>>) {
        // Drain the auth channel and append the messages to the outgoing events
        outgoing_messages.append(&mut self.incoming_messages);
    }

    pub(crate) fn buffer_pop_front_until_and_including(&mut self, id: MessageIndex) {
        self.buffered_messages.pop_front_until_and_including(id);
    }

    #[allow(dead_code)]
    pub(crate) fn buffer_pop_front_until_and_excluding(&mut self, id: MessageIndex) {
        self.buffered_messages.pop_front_until_and_excluding(id);
    }

    pub(crate) fn receive_message(
        &mut self,
        entity_state_opt: Option<EntityChannelState>,
        id: MessageIndex,
        msg: EntityMessage<()>,
    ) {
        self.buffered_messages.push_back(id, msg);
        self.process_messages(entity_state_opt);
    }

    pub(crate) fn process_messages(&mut self, entity_state_opt: Option<EntityChannelState>) {
        if let Some(entity_state) = entity_state_opt {
            if entity_state != EntityChannelState::Spawned {
                // If the entity is not spawned, we do not process any messages
                return;
            }
        }

        loop {
            let Some((_, msg)) = self.buffered_messages.peek_front() else {
                break;
            };

            let Some(subcommand_id) = msg.subcommand_id() else {
                panic!("Expected a subcommand ID in the message: {:?}", msg);
            };

            if subcommand_id != self.next_subcommand_id {
                // If the subcommand ID does not match the next expected ID, we stop processing
                break;
            }

            // Move to the next expected subcommand ID
            self.next_subcommand_id = self.next_subcommand_id.wrapping_add(1);

            let (_, msg) = self.buffered_messages.pop_front().unwrap();

            self.incoming_messages.push(msg);
        }
    }

    #[cfg(feature = "e2e_debug")]
    pub(crate) fn debug_diagnostic(&self) -> (SubCommandId, usize, Option<SubCommandId>, usize) {
        let head_sub_id = self
            .buffered_messages
            .peek_front()
            .and_then(|(_, msg)| msg.subcommand_id());
        let buffer_len = self.buffered_messages.len();
        let incoming_len = self.incoming_messages.len();
        (
            self.next_subcommand_id,
            buffer_len,
            head_sub_id,
            incoming_len,
        )
    }
}
