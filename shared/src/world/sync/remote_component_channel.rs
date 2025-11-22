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
            received_messages.push(msg_type.with_component_kind(&component_kind));
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

        loop {
            let Some((id, insert)) = self.buffered_messages.peek_front() else {
                break;
            };

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
