use std::{any::TypeId, collections::HashMap, marker::PhantomData, ops::Deref};

use hecs::World;

use naia_server::{ImplRef, EntityType, ProtocolType, Ref, Replicate, WorldType};

use super::{component_access::ComponentAccess, entity::Entity};

pub struct WorldData<P: ProtocolType> {
    rep_type_to_handler_map: HashMap<TypeId, Box<dyn ComponentAccess<P>>>,
    ref_type_to_rep_type_map: HashMap<TypeId, TypeId>,
}

impl<P: ProtocolType> WorldData<P> {
    pub fn new() -> Self {
        WorldData {
            rep_type_to_handler_map: HashMap::new(),
            ref_type_to_rep_type_map: HashMap::new(),
        }
    }
}