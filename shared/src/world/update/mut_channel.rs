use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    sync::{Arc, OnceLock, RwLock, RwLockReadGuard, Weak},
};

use crate::{ComponentKind, DiffMask, GlobalEntity, GlobalWorldManagerType, PropertyMutate};

/// Shared dirty-component set owned by a UserDiffHandler. MutReceivers hold a
/// Weak pointer into it and push their own (entity, kind) key whenever their
/// diff mask transitions clean → dirty (or pop it on the reverse transition).
/// This lets `dirty_receiver_candidates` read the set in O(dirty) instead of
/// scanning every receiver every tick — the core of the Phase-3 win.
pub type DirtySet = RwLock<HashMap<GlobalEntity, HashSet<ComponentKind>>>;

/// Identifies a MutReceiver's position inside its owning UserDiffHandler's
/// dirty set. Installed once per receiver via `MutReceiver::attach_notifier`
/// (OnceLock — all clones share the notifier).
pub struct DirtyNotifier {
    entity: GlobalEntity,
    kind: ComponentKind,
    set: Weak<DirtySet>,
}

impl DirtyNotifier {
    pub fn new(entity: GlobalEntity, kind: ComponentKind, set: Weak<DirtySet>) -> Self {
        Self { entity, kind, set }
    }

    fn notify_dirty(&self) {
        if let Some(set) = self.set.upgrade() {
            if let Ok(mut guard) = set.write() {
                guard.entry(self.entity).or_default().insert(self.kind);
            }
        }
    }

    fn notify_clean(&self) {
        if let Some(set) = self.set.upgrade() {
            if let Ok(mut guard) = set.write() {
                if let Some(kinds) = guard.get_mut(&self.entity) {
                    kinds.remove(&self.kind);
                    if kinds.is_empty() {
                        guard.remove(&self.entity);
                    }
                }
            }
        }
    }
}

pub trait MutChannelType: Send + Sync {
    fn new_receiver(&mut self, address: &Option<SocketAddr>) -> Option<MutReceiver>;
    fn send(&self, diff: u8);
}

// MutChannel
#[derive(Clone)]
pub struct MutChannel {
    data: Arc<RwLock<dyn MutChannelType>>,
}

impl MutChannel {
    pub fn new_channel(
        global_world_manager: &dyn GlobalWorldManagerType,
        diff_mask_length: u8,
    ) -> (MutSender, MutReceiverBuilder) {
        let channel = Self {
            data: global_world_manager.new_mut_channel(diff_mask_length),
        };

        let sender = channel.new_sender();

        let builder = MutReceiverBuilder::new(&channel);

        (sender, builder)
    }

    pub fn new_sender(&self) -> MutSender {
        MutSender::new(self)
    }

    pub fn new_receiver(&self, address: &Option<SocketAddr>) -> Option<MutReceiver> {
        if let Ok(mut data) = self.data.as_ref().write() {
            return data.new_receiver(address);
        }
        None
    }

    pub fn send(&self, property_index: u8) -> bool {
        if let Ok(data) = self.data.as_ref().read() {
            data.send(property_index);
            return true;
        }
        false
    }
}

// MutReceiver
#[derive(Clone)]
pub struct MutReceiver {
    mask: Arc<RwLock<DiffMask>>,
    notifier: Arc<OnceLock<DirtyNotifier>>,
}

impl MutReceiver {
    pub fn new(diff_mask_length: u8) -> Self {
        Self {
            mask: Arc::new(RwLock::new(DiffMask::new(diff_mask_length))),
            notifier: Arc::new(OnceLock::new()),
        }
    }

    /// Installed once per receiver by UserDiffHandler::register_component.
    /// Cheap no-op on re-attachment (OnceLock::set returns Err, ignored).
    pub fn attach_notifier(&self, notifier: DirtyNotifier) {
        let _ = self.notifier.set(notifier);
    }

    pub fn mask(&'_ self) -> RwLockReadGuard<'_, DiffMask> {
        let Ok(mask) = self.mask.as_ref().read() else {
            panic!("Mask held on current thread");
        };

        mask
    }

    pub fn diff_mask_is_clear(&self) -> bool {
        let Ok(mask) = self.mask.as_ref().read() else {
            panic!("Mask held on current thread");
        };
        return mask.is_clear();
    }

    pub fn mutate(&self, property_index: u8) {
        let Ok(mut mask) = self.mask.as_ref().write() else {
            panic!("Mask held on current thread");
        };
        let was_clear = mask.is_clear();
        mask.set_bit(property_index, true);
        if was_clear {
            if let Some(n) = self.notifier.get() {
                n.notify_dirty();
            }
        }
    }

    pub fn or_mask(&self, other_mask: &DiffMask) {
        let Ok(mut mask) = self.mask.as_ref().write() else {
            panic!("Mask held on current thread");
        };
        let was_clear = mask.is_clear();
        mask.or(other_mask);
        if was_clear && !mask.is_clear() {
            if let Some(n) = self.notifier.get() {
                n.notify_dirty();
            }
        }
    }

    pub fn clear_mask(&self) {
        let Ok(mut mask) = self.mask.as_ref().write() else {
            panic!("Mask held on current thread");
        };
        let was_clear = mask.is_clear();
        mask.clear();
        if !was_clear {
            if let Some(n) = self.notifier.get() {
                n.notify_clean();
            }
        }
    }
}

// MutSender
#[derive(Clone)]
pub struct MutSender {
    channel: MutChannel,
}

impl MutSender {
    pub fn new(channel: &MutChannel) -> Self {
        Self {
            channel: channel.clone(),
        }
    }
}

impl PropertyMutate for MutSender {
    fn mutate(&mut self, property_index: u8) -> bool {
        let success = self.channel.send(property_index);
        success
    }
}

// MutReceiverBuilder
pub struct MutReceiverBuilder {
    channel: MutChannel,
}

impl MutReceiverBuilder {
    pub fn new(channel: &MutChannel) -> Self {
        Self {
            channel: channel.clone(),
        }
    }

    pub fn build(&self, address: &Option<SocketAddr>) -> Option<MutReceiver> {
        self.channel.new_receiver(address)
    }
}
