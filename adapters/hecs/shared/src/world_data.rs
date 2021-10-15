use std::{
    any::{Any, TypeId},
    collections::HashMap,
};

use naia_shared::ProtocolType;

use super::component_access::{ComponentAccess, ComponentAccessor};

#[derive(Debug)]
pub struct WorldData {
    rep_type_to_accessor_map: HashMap<TypeId, Box<dyn Any>>,
    ref_type_to_rep_type_map: HashMap<TypeId, TypeId>,
}

impl WorldData {
    pub fn new() -> Self {
        WorldData {
            rep_type_to_accessor_map: HashMap::new(),
            ref_type_to_rep_type_map: HashMap::new(),
        }
    }

    pub(crate) fn get_component_access<P: ProtocolType>(
        &self,
        type_id: &TypeId,
    ) -> Option<&Box<dyn ComponentAccess<P>>> {
        if let Some(accessor_any) = self.rep_type_to_accessor_map.get(type_id) {
            return accessor_any.downcast_ref::<Box<dyn ComponentAccess<P>>>();
        }
        return None;
    }

    pub(crate) fn has_type(&self, type_id: &TypeId) -> bool {
        return self.rep_type_to_accessor_map.contains_key(type_id);
    }

    pub(crate) fn put_type<P: ProtocolType, R: Replicate<P>>(
        &mut self,
        rep_type_id: &TypeId,
        ref_type_id: &TypeId,
    ) {
        self.rep_type_to_accessor_map
            .insert(*rep_type_id, ComponentAccessor::<P, R>::new());
        self.ref_type_to_rep_type_map
            .insert(*ref_type_id, *rep_type_id);
    }

    pub(crate) fn type_convert_ref_to_rep(&self, ref_type_id: &TypeId) -> Option<&TypeId> {
        return self.ref_type_to_rep_type_map.get(ref_type_id);
    }
}

unsafe impl Send for WorldData {}
unsafe impl Sync for WorldData {}
