//! # `ComponentChannel` – Per‑Component idempotent FSM
//!
//! This module owns the **insert / remove** lifecycle for a *single*
//! component type (`ComponentKind`) on a *single* entity.  Its job is to
//! translate an **unordered** stream of
//! `EntityMessage::{InsertComponent, RemoveComponent}` into a *locally
//! ordered, idempotent* stream that the ECS can apply safely.
//!
//! ## Why a dedicated channel?
//! * **Locality** – Ordering is only meaningful *within the scope of one
//!   component on one entity*; isolating that scope lets unrelated
//!   components proceed even if this one stalls.
//! * **HoLB elimination** – By buffering at this granularity we avoid a
//!   stale component update blocking the entire entity.
//!
//! ## State machine
//! ```text
//! [inserted = false] --Insert→ [inserted = true]
//! [inserted = true ] --Remove→ [inserted = false]
//! ```
//! Invalid transitions (e.g. Insert when `inserted = true`) are *buffered*
//! until an intervening Remove makes them legal, or discarded if their
//! `MessageIndex` is ≤ `last_insert_id` (wrap‑around‑safe comparison via
//! `sequence_equal_or_less_than`).
//!
//! The result is an **at‑most‑once, causally ordered** stream of
//! component‑level events, ready for `EntityChannel` to forward once the
//! parent entity itself is confirmed `Spawned`.
//!
//! **Contract**: Every `InsertComponent` emitted by this channel is the
//! *earliest not‑yet‑applied* insertion for that component, and every
//! `RemoveComponent` is the matching inverse, guaranteeing the ECS sees a
//! consistent on/off toggle without duplicates or reversals.

use std::collections::VecDeque;

use crate::world::sync::ordered_ids::OrderedIds;
use crate::{
    sequence_equal_or_less_than, world::sync::remote_entity_channel::EntityChannelState,
    ComponentKind, EntityMessage, EntityMessageType, MessageIndex,
};

pub(crate) struct RemoteComponentChannel {
    /// Current authoritative presence flag
    inserted: bool,
    /// The *newest* message that was valid; guards against replay / re‑order.
    last_epoch_id: Option<MessageIndex>,
    /// Small ring of *pending* insert (`true`) / remove (`false`) flags keyed by their sequence IDs.
    buffered_messages: OrderedIds<bool>,
    incoming_messages: VecDeque<EntityMessageType>,
}

impl RemoteComponentChannel {
    pub(crate) fn new() -> Self {
        Self {
            inserted: false,
            last_epoch_id: None,
            buffered_messages: OrderedIds::new(),
            incoming_messages: VecDeque::new(),
        }
    }

    pub(crate) fn drain_messages_into(
        &mut self,
        component_kind: &ComponentKind,
        outgoing_messages: &mut Vec<EntityMessage<()>>,
    ) {
        // Drain the component channel and append the messages to the outgoing events
        let mut received_messages = Vec::new();
        for msg_type in std::mem::take(&mut self.incoming_messages) {
            received_messages.push(msg_type.with_component_kind(component_kind));
        }
        outgoing_messages.append(&mut received_messages);
    }

    pub(crate) fn buffer_pop_front_until_and_excluding(&mut self, id: MessageIndex) {
        self.buffered_messages.pop_front_until_and_excluding(id);
    }

    pub(crate) fn accept_message(
        &mut self,
        entity_state: EntityChannelState,
        id: MessageIndex,
        msg: EntityMessage<()>,
    ) {
        if let Some(last_epoch_id) = self.last_epoch_id {
            if sequence_equal_or_less_than(id, last_epoch_id) {
                // This message is older than the last insert message, ignore it
                return;
            }
        }

        let insert = match &msg {
            EntityMessage::InsertComponent(_, _) => true,
            EntityMessage::RemoveComponent(_, _) => false,
            _ => panic!(
                "ComponentChannel can only accept InsertComponent or RemoveComponent messages"
            ),
        };

        self.buffered_messages.push_back(id, insert);

        self.process_messages(entity_state);
    }

    pub(crate) fn process_messages(&mut self, entity_state: EntityChannelState) {
        if entity_state != EntityChannelState::Spawned {
            // If the entity is not spawned, we cannot process any messages
            return;
        }

        while let Some((id, insert)) = self.buffered_messages.peek_front() {
            let id = *id;

            match *insert {
                true => {
                    if self.inserted {
                        break;
                    }
                    self.set_inserted(true, id);
                }
                false => {
                    if !self.inserted {
                        break;
                    }
                    self.set_inserted(false, id);
                }
            }

            let (_, insert) = self.buffered_messages.pop_front().unwrap();
            if insert {
                self.incoming_messages
                    .push_back(EntityMessageType::InsertComponent);
            } else {
                self.incoming_messages
                    .push_back(EntityMessageType::RemoveComponent);
            }
        }
    }

    pub(crate) fn set_inserted(&mut self, inserted: bool, last_epoch_id: MessageIndex) {
        self.inserted = inserted;
        self.last_epoch_id = Some(last_epoch_id);
    }

    pub(crate) fn is_inserted(&self) -> bool {
        self.inserted
    }

    pub(crate) fn force_drain_buffers(&mut self, _entity_state: EntityChannelState) {
        // Force-drain all buffered operations regardless of FSM state
        while let Some((id, insert)) = self.buffered_messages.pop_front() {
            if insert {
                self.incoming_messages
                    .push_back(EntityMessageType::InsertComponent);
            } else {
                self.incoming_messages
                    .push_back(EntityMessageType::RemoveComponent);
            }
            // Update the inserted state to reflect the final operation
            self.inserted = insert;
            self.last_epoch_id = Some(id);
        }
    }
}

#[cfg(test)]
mod tests {
    //! The per-component insert/remove FSM had no direct tests. A sweep found
    //! `buffer_pop_front_until_and_excluding` and `force_drain_buffers` could
    //! each be replaced with `()` unnoticed -- the first is how a spawn discards
    //! a previous lifetime's backlog, the second is the migration escape hatch
    //! that empties the buffer regardless of FSM state.
    //!
    //! Assertions read `incoming_messages` directly rather than going through
    //! `drain_messages_into`, which would need a real `Replicate` type just to
    //! name a `ComponentKind`.

    use std::any::TypeId;

    use super::*;

    fn kind() -> ComponentKind {
        ComponentKind::from(TypeId::of::<u8>())
    }

    fn insert_msg() -> EntityMessage<()> {
        EntityMessage::InsertComponent((), kind())
    }

    fn remove_msg() -> EntityMessage<()> {
        EntityMessage::RemoveComponent((), kind())
    }

    fn emitted(channel: &RemoteComponentChannel) -> Vec<EntityMessageType> {
        channel.incoming_messages.iter().copied().collect()
    }

    #[test]
    fn an_insert_then_a_remove_toggles_the_component() {
        let mut channel = RemoteComponentChannel::new();

        channel.accept_message(EntityChannelState::Spawned, 1, insert_msg());
        assert!(channel.is_inserted());

        channel.accept_message(EntityChannelState::Spawned, 2, remove_msg());
        assert!(!channel.is_inserted());

        assert_eq!(
            emitted(&channel),
            vec![
                EntityMessageType::InsertComponent,
                EntityMessageType::RemoveComponent,
            ],
        );
    }

    /// Nothing is applied while the parent entity is unspawned: the component
    /// stream is gated on the entity's own spawn barrier.
    #[test]
    fn messages_are_buffered_until_the_entity_spawns() {
        let mut channel = RemoteComponentChannel::new();

        channel.accept_message(EntityChannelState::Despawned, 1, insert_msg());
        assert!(emitted(&channel).is_empty());
        assert!(!channel.is_inserted());

        channel.process_messages(EntityChannelState::Spawned);

        assert_eq!(emitted(&channel), vec![EntityMessageType::InsertComponent]);
        assert!(channel.is_inserted());
    }

    /// An out-of-order Remove arriving before the entity is inserted stalls at
    /// the front of the buffer rather than being applied or dropped -- and the
    /// later Insert behind it stays stalled too, since applying it first would
    /// reverse the pair.
    #[test]
    fn an_illegal_transition_stalls_the_buffer_instead_of_applying() {
        let mut channel = RemoteComponentChannel::new();

        channel.accept_message(EntityChannelState::Spawned, 1, remove_msg());

        assert!(emitted(&channel).is_empty());
        assert!(!channel.is_inserted());
    }

    #[test]
    fn a_replayed_message_is_ignored() {
        let mut channel = RemoteComponentChannel::new();
        channel.accept_message(EntityChannelState::Spawned, 5, insert_msg());
        channel.incoming_messages.clear();

        // Older than the last applied epoch: a duplicate from the network.
        channel.accept_message(EntityChannelState::Spawned, 3, remove_msg());

        assert!(emitted(&channel).is_empty(), "a stale replay was applied");
        assert!(channel.is_inserted());
    }

    /// The spawn barrier discards component messages from a previous lifetime
    /// of the entity. `_excluding` means the spawn's own id survives.
    #[test]
    fn popping_the_buffer_discards_pre_spawn_messages() {
        let mut channel = RemoteComponentChannel::new();
        channel.accept_message(EntityChannelState::Despawned, 1, insert_msg());

        channel.buffer_pop_front_until_and_excluding(5);
        channel.process_messages(EntityChannelState::Spawned);

        assert!(
            emitted(&channel).is_empty(),
            "a message from before the spawn survived and was applied",
        );
        assert!(!channel.is_inserted());
    }

    /// The other direction, so the test above cannot pass against a channel
    /// that simply never applies anything: below the boundary the message is
    /// kept and applied as normal.
    #[test]
    fn popping_the_buffer_keeps_messages_at_or_past_the_boundary() {
        let mut channel = RemoteComponentChannel::new();
        channel.accept_message(EntityChannelState::Despawned, 5, insert_msg());

        channel.buffer_pop_front_until_and_excluding(5);
        channel.process_messages(EntityChannelState::Spawned);

        assert_eq!(emitted(&channel), vec![EntityMessageType::InsertComponent]);
    }

    /// `force_drain_buffers` is the migration escape hatch: it empties the
    /// buffer regardless of what the FSM would allow. The stalled Remove from
    /// `an_illegal_transition_stalls_the_buffer_instead_of_applying` is exactly
    /// what it has to get out.
    #[test]
    fn force_draining_emits_operations_the_fsm_would_have_stalled() {
        let mut channel = RemoteComponentChannel::new();
        channel.accept_message(EntityChannelState::Spawned, 1, remove_msg());
        assert!(
            emitted(&channel).is_empty(),
            "fixture: the remove should stall"
        );

        channel.force_drain_buffers(EntityChannelState::Spawned);

        assert_eq!(
            emitted(&channel),
            vec![EntityMessageType::RemoveComponent],
            "the stalled operation was not force-drained",
        );
    }

    /// After a force drain the channel's presence flag must reflect the LAST
    /// operation drained, not the first -- otherwise the channel disagrees with
    /// the ECS about whether the component is there.
    #[test]
    fn force_draining_leaves_the_state_of_the_final_operation() {
        let mut channel = RemoteComponentChannel::new();
        channel.accept_message(EntityChannelState::Despawned, 1, insert_msg());
        channel.accept_message(EntityChannelState::Despawned, 2, remove_msg());
        channel.accept_message(EntityChannelState::Despawned, 3, insert_msg());

        channel.force_drain_buffers(EntityChannelState::Despawned);

        assert_eq!(
            emitted(&channel),
            vec![
                EntityMessageType::InsertComponent,
                EntityMessageType::RemoveComponent,
                EntityMessageType::InsertComponent,
            ],
        );
        assert!(
            channel.is_inserted(),
            "the final drained operation was an insert, so the component is present",
        );
    }

    #[test]
    fn force_draining_an_empty_buffer_emits_nothing() {
        let mut channel = RemoteComponentChannel::new();

        channel.force_drain_buffers(EntityChannelState::Spawned);

        assert!(emitted(&channel).is_empty());
    }
}
