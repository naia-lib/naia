use crate::{world::host::world_channel::CheckedMap, ComponentKind};

// ComponentChannel

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum ComponentChannel {
    Inserting,
    Inserted,
    Removing,
}

// EntityChannel

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
enum EntityChannelState {
    Spawning,
    Spawned,
    Despawning,
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum ReleaseAuthState {
    None,
    Waiting,
    Complete,
}

pub struct EntityChannel {
    components: CheckedMap<ComponentKind, ComponentChannel>,
    state: EntityChannelState,
    release_auth: ReleaseAuthState,
    messages_in_progress: u8,
    despawn_after_spawned: bool,
}

impl EntityChannel {
    pub fn new_spawning() -> Self {
        Self {
            components: CheckedMap::new(),
            state: EntityChannelState::Spawning,
            release_auth: ReleaseAuthState::None,
            messages_in_progress: 0,
            despawn_after_spawned: false,
        }
    }

    // this may be used for tracking remote entities which are sure to be spawned on the remote already
    pub fn new_spawned() -> Self {
        Self {
            components: CheckedMap::new(),
            state: EntityChannelState::Spawned,
            release_auth: ReleaseAuthState::None,
            messages_in_progress: 0,
            despawn_after_spawned: false,
        }
    }

    pub(crate) fn is_spawned(&self) -> bool {
        return self.state == EntityChannelState::Spawned;
    }

    pub(crate) fn is_spawning(&self) -> bool {
        return self.state == EntityChannelState::Spawning;
    }

    pub(crate) fn is_despawning(&self) -> bool {
        return self.state == EntityChannelState::Despawning;
    }

    pub(crate) fn inserted_components(&self) -> Vec<ComponentKind> {
        let mut output = Vec::new();

        for (component_kind, component_channel) in self.components.iter() {
            if *component_channel == ComponentChannel::Inserted {
                output.push(*component_kind);
            }
        }

        output
    }

    pub(crate) fn has_component(&self, component_kind: &ComponentKind) -> bool {
        return self.components.contains_key(component_kind);
    }

    pub(crate) fn component_is_inserting(&self, component_kind: &ComponentKind) -> bool {
        if let Some(component_channel) = self.components.get(component_kind) {
            return *component_channel == ComponentChannel::Inserting;
        }
        return false;
    }

    pub(crate) fn queue_despawn_after_spawned(&mut self) {
        self.despawn_after_spawned = true;
    }

    // returns (if entity should be immediately despawned, if release message should be sent)
    pub(crate) fn spawning_complete(&mut self) -> (bool, bool) {
        if self.state != EntityChannelState::Spawning {
            panic!("EntityChannel::spawning_complete() called on non-spawning entity");
        }
        self.state = EntityChannelState::Spawned;

        let mut release_message_queued = false;
        if self.ready_to_release() && self.release_auth == ReleaseAuthState::Waiting {
            self.release_auth = ReleaseAuthState::Complete;
            release_message_queued = true;
        }

        if self.components.len() > 0 {
            panic!("Newly spawned entity should not have any components yet..");
        }

        return (self.despawn_after_spawned, release_message_queued);
    }

    pub(crate) fn despawn(&mut self) {
        if self.state != EntityChannelState::Spawned {
            panic!("EntityChannel::despawn() called on non-spawned entity");
        }
        self.state = EntityChannelState::Despawning;
        self.components.clear();
    }

    pub(crate) fn component_is_removing(&self, component_kind: &ComponentKind) -> bool {
        if let Some(component_channel) = self.components.get(component_kind) {
            return *component_channel == ComponentChannel::Removing;
        }
        return false;
    }

    pub(crate) fn insert_component(&mut self, component_kind: &ComponentKind, after_spawn: bool) {
        if self.state != EntityChannelState::Spawned {
            panic!("should only be inserting components into spawned entities");
        }
        self.components
            .insert(*component_kind, ComponentChannel::Inserting);
        self.send_message(after_spawn);
    }

    pub(crate) fn insert_remote_component(&mut self, component_kind: &ComponentKind) {
        self.components
            .insert(*component_kind, ComponentChannel::Inserted);
    }

    pub(crate) fn remove_component(&mut self, component_kind: &ComponentKind) -> bool {
        match self.components.get(component_kind) {
            Some(ComponentChannel::Inserted) => {
                self.components.remove(component_kind);
                self.components
                    .insert(*component_kind, ComponentChannel::Removing);
                self.send_message(false);
                return true;
            }
            Some(ComponentChannel::Inserting) => {
                todo!();
            }
            _ => {
                return false;
            }
        }
    }

    // returns whether auth release message should be sent
    pub(crate) fn component_insertion_complete(&mut self, component_kind: &ComponentKind) -> bool {
        if let Some(component_channel) = self.components.get_mut(component_kind) {
            if *component_channel == ComponentChannel::Inserting {
                *component_channel = ComponentChannel::Inserted;
                return self.message_delivered();
            }
        }

        panic!("component_insertion_complete called on non-inserting component?");
    }

    // returns whether auth release message should be sent
    pub(crate) fn component_removal_complete(&mut self, component_kind: &ComponentKind) -> bool {
        if self.components.get(component_kind) == Some(&ComponentChannel::Removing) {
            self.components.remove(component_kind);
            return self.message_delivered();
        } else {
            panic!("component_removal_complete called on non-removing component");
        }
    }

    fn send_message(&mut self, after_spawn: bool) {
        if self.release_auth == ReleaseAuthState::None || after_spawn {
            self.messages_in_progress += 1;
        } else {
            panic!("Entity channel should be blocked right now, as auth has been released");
        }
    }

    fn message_delivered(&mut self) -> bool {
        self.messages_in_progress -= 1;

        if self.ready_to_release() && self.release_auth == ReleaseAuthState::Waiting {
            self.release_auth = ReleaseAuthState::Complete;
            return true;
        }

        return false;
    }

    // // returns whether auth release message should be sent
    pub(crate) fn release_authority(&mut self) -> bool {
        if self.ready_to_release() {
            self.release_auth = ReleaseAuthState::Complete;
            return true;
        } else {
            self.release_auth = ReleaseAuthState::Waiting;
            return false;
        }
    }

    fn ready_to_release(&self) -> bool {
        self.messages_in_progress == 0 && self.state == EntityChannelState::Spawned
    }
}

impl Drop for EntityChannel {
    fn drop(&mut self) {
        if self.release_auth == ReleaseAuthState::Waiting {
            panic!("Entity Channel Auth Release message was waiting, but is now dropped");
        }
    }
}
