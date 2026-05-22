use std::{
    collections::HashMap,
    time::Duration,
};

use crate::{ComponentKind, DiffMask, GlobalEntity, Instant, PacketIndex};

const DROP_UPDATE_RTT_FACTOR: f32 = 1.5;

type SentUpdatesMap =
    HashMap<PacketIndex, (Instant, HashMap<(GlobalEntity, ComponentKind), DiffMask>)>;

/// Worker-owned retransmit machinery, carved out of `EntityUpdateManager`
/// (MISSION_TICK_FLOOR Lever 3 / L3 send-state seam).
///
/// Holds the per-packet `sent_updates` ledger and the most-recently-sent
/// packet index. This is purely *transmit* state: the send worker records what
/// was put on the wire (`record_sent_update`), the ACK drain removes delivered
/// packets (`notify_packet_delivered`), and the drop detector folds the masks
/// of un-acked-and-timed-out packets back into the candidate set
/// (`collect_dropped_masks`). It deliberately holds NO diff-state: the dropped
/// masks are returned to the caller, which re-sets them on the (shared) diff
/// ledger via an atomic `or`.
pub struct RetransmitLedger {
    sent_updates: SentUpdatesMap,
    last_update_packet_index: PacketIndex,
}

impl RetransmitLedger {
    pub fn new() -> Self {
        Self {
            sent_updates: HashMap::new(),
            last_update_packet_index: 0,
        }
    }

    /// Record the per-packet `sent_updates` ledger entry for a component update
    /// just written into `packet_index`. Does NOT touch any diff mask — on the
    /// server send path the live mask is cleared up-front in
    /// `SendState::prepare_send_job`; the client clears separately (see
    /// `LocalWorldManager::record_update`).
    pub fn record_sent_update(
        &mut self,
        now: &Instant,
        packet_index: &PacketIndex,
        global_entity: &GlobalEntity,
        component_kind: &ComponentKind,
        diff_mask: DiffMask,
    ) {
        self.last_update_packet_index = *packet_index;

        if !self.sent_updates.contains_key(packet_index) {
            self.sent_updates
                .insert(*packet_index, (now.clone(), HashMap::new()));
        }
        let (_, sent_updates_map) = self.sent_updates.get_mut(packet_index).unwrap();
        sent_updates_map.insert((*global_entity, *component_kind), diff_mask);
    }

    /// Drop a delivered packet's record (called from the ACK drain).
    pub fn notify_packet_delivered(&mut self, packet_index: PacketIndex) {
        self.sent_updates.remove(&packet_index);
    }

    /// Detect packets that timed out (un-acked beyond the RTT-scaled drop
    /// window), remove their ledger entries, and return the
    /// `(entity, component, folded_mask)` set the caller must re-dirty.
    ///
    /// The folded mask NANDs out any bits that a *later* still-tracked packet
    /// already carried (the NACK-replay semantics), so only fields that were
    /// genuinely lost are re-sent. This reads only retransmit state — the
    /// `has_component` filter and the atomic mask `or` are applied by the
    /// caller against the diff ledger.
    pub fn collect_dropped_masks(
        &mut self,
        now: &Instant,
        rtt_millis: &f32,
    ) -> Vec<(GlobalEntity, ComponentKind, DiffMask)> {
        let drop_duration = Duration::from_millis((DROP_UPDATE_RTT_FACTOR * rtt_millis) as u64);

        let mut dropped_packets = Vec::new();
        for (packet_index, (time_sent, _)) in &self.sent_updates {
            let elapsed_since_send = time_sent.elapsed(now);
            if elapsed_since_send > drop_duration {
                dropped_packets.push(*packet_index);
            }
        }

        let mut out = Vec::new();
        for dropped_packet_index in dropped_packets {
            if let Some((_, diff_mask_map)) = self.sent_updates.remove(&dropped_packet_index) {
                for (component_index, diff_mask) in &diff_mask_map {
                    let (entity, component) = component_index;
                    let mut new_diff_mask = diff_mask.clone();

                    // walk from dropped packet up to most recently sent packet
                    if dropped_packet_index != self.last_update_packet_index {
                        let mut packet_index = dropped_packet_index.wrapping_add(1);
                        while packet_index != self.last_update_packet_index {
                            if let Some((_, diff_mask_map)) = self.sent_updates.get(&packet_index) {
                                if let Some(next_diff_mask) = diff_mask_map.get(component_index) {
                                    new_diff_mask.nand(next_diff_mask);
                                }
                            }

                            packet_index = packet_index.wrapping_add(1);
                        }
                    }

                    out.push((*entity, *component, new_diff_mask));
                }
            }
        }
        out
    }
}
