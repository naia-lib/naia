
use crate::{world::host::world_channel::CheckedMap, ComponentKind};

// ComponentChannel

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum ComponentChannel {
    Inserting,
    Inserted,
    Removing,
}

// EntityChannel

#[derive(PartialEq, Eq, Clone, Copy)]
enum EntityChannelState {
    Spawning,
    Spawned,
    Despawning,
}

pub struct EntityChannel {
    components: CheckedMap<ComponentKind, ComponentChannel>,
    state: EntityChannelState,
}

impl EntityChannel {
    pub fn new_spawning() -> Self {
        Self {
            components: CheckedMap::new(),
            state: EntityChannelState::Spawning,
        }
    }

    pub fn new_spawned() -> Self {
        Self {
            components: CheckedMap::new(),
            state: EntityChannelState::Spawned,
        }
    }

    pub fn new_despawning() -> Self {
        Self {
            components: CheckedMap::new(),
            state: EntityChannelState::Despawning,
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

    pub(crate) fn insert_component(&mut self, component_kind: &ComponentKind) {
        self.components.insert(*component_kind, ComponentChannel::Inserting);
    }

    pub(crate) fn insert_remote_component(&mut self, component_kind: &ComponentKind) {
        self.components.insert(*component_kind, ComponentChannel::Inserted);
    }

    pub(crate) fn remove_component(&mut self, component_kind: &ComponentKind) -> bool {
        match self.components.get(component_kind) {
            Some(ComponentChannel::Inserted) => {
                self.components.remove(component_kind);
                self.components.insert(*component_kind, ComponentChannel::Removing);
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

    pub(crate) fn component_is_inserting(&self, component_kind: &ComponentKind) -> bool {
        if let Some(component_channel) = self.components.get(component_kind) {
            return *component_channel == ComponentChannel::Inserting;
        }
        return false;
    }

    pub(crate) fn component_insertion_complete(&mut self, component_kind: &ComponentKind) {
        if let Some(component_channel) = self.components.get_mut(component_kind) {
            if *component_channel == ComponentChannel::Inserting {
                *component_channel = ComponentChannel::Inserted;
            }
        }
    }

    pub(crate) fn component_is_removing(&self, component_kind: &ComponentKind) -> bool {
        if let Some(component_channel) = self.components.get(component_kind) {
            return *component_channel == ComponentChannel::Removing;
        }
        return false;
    }

    pub(crate) fn component_removal_complete(&mut self, component_kind: &ComponentKind) {
        if self.components.get(component_kind) == Some(&ComponentChannel::Removing) {
            self.components.remove(component_kind);
        } else {
            panic!("component_removal_complete called on non-removing component");
        }
    }
}