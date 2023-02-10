use std::{any::Any, collections::HashMap};

use naia_shared::{ComponentKind, Replicate};

use super::component_access::{ComponentAccess, ComponentAccessor};

pub struct WorldData {
    kind_to_accessor_map: HashMap<ComponentKind, Box<dyn Any>>,
}

impl Default for WorldData {
    fn default() -> Self {
        Self {
            kind_to_accessor_map: HashMap::default(),
        }
    }
}

impl WorldData {
    pub fn new() -> Self {
        WorldData {
            kind_to_accessor_map: HashMap::new(),
        }
    }

    #[allow(clippy::borrowed_box)]
    pub(crate) fn component_access(
        &self,
        component_kind: &ComponentKind,
    ) -> Option<&Box<dyn ComponentAccess>> {
        if let Some(accessor_any) = self.kind_to_accessor_map.get(component_kind) {
            return accessor_any.downcast_ref::<Box<dyn ComponentAccess>>();
        }
        None
    }

    pub(crate) fn put_kind<R: Replicate>(&mut self, component_kind: &ComponentKind) {
        self.kind_to_accessor_map
            .insert(*component_kind, ComponentAccessor::<R>::create());
    }
}

unsafe impl Send for WorldData {}
unsafe impl Sync for WorldData {}
