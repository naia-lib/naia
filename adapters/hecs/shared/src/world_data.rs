use std::{any::Any, collections::HashMap};

use naia_shared::{Protocolize, ReplicateSafe};

use super::component_access::{ComponentAccess, ComponentAccessor};

pub struct WorldData<P: Protocolize> {
    kind_to_accessor_map: HashMap<P::Kind, Box<dyn Any>>,
}

impl<P: Protocolize> Default for WorldData<P> {
    fn default() -> Self {
        Self {
            kind_to_accessor_map: HashMap::default(),
        }
    }
}

impl<P: Protocolize> WorldData<P> {
    pub fn new() -> Self {
        WorldData {
            kind_to_accessor_map: HashMap::new(),
        }
    }

    #[allow(clippy::borrowed_box)]
    pub(crate) fn component_access(
        &self,
        component_kind: &P::Kind,
    ) -> Option<&Box<dyn ComponentAccess<P>>> {
        if let Some(accessor_any) = self.kind_to_accessor_map.get(component_kind) {
            return accessor_any.downcast_ref::<Box<dyn ComponentAccess<P>>>();
        }
        None
    }

    pub(crate) fn has_kind(&self, component_kind: &P::Kind) -> bool {
        self.kind_to_accessor_map.contains_key(component_kind)
    }

    pub(crate) fn put_kind<R: ReplicateSafe<P>>(&mut self, component_kind: &P::Kind) {
        self.kind_to_accessor_map
            .insert(*component_kind, ComponentAccessor::<P, R>::create());
    }
}

unsafe impl<P: Protocolize> Send for WorldData<P> {}
unsafe impl<P: Protocolize> Sync for WorldData<P> {}
