use std::{any::TypeId, collections::HashMap, sync::Mutex};

use lazy_static::lazy_static;

use naia_serde::{BitReader, BitWrite, SerdeErr};

use crate::{
    component::{
        component_update::ComponentUpdate,
        diff_mask::DiffMask,
        property_mutate::PropertyMutator,
        replica_ref::{ReplicaDynMut, ReplicaDynRef},
    },
    entity::{entity_handle::EntityHandle, entity_property::NetEntityHandleConverter},
    messages::named::Named,
    types::ComponentId,
};

/// A map to hold all component types
pub struct Components;

impl Components {

    pub fn add_component<C: Replicate>() {
        let mut components_data = COMPONENTS_DATA.lock().unwrap();
        let type_id = TypeId::of::<C>();
        let component_id = ComponentId::new(components_data.current_id);
        components_data.type_to_id_map.insert(type_id, component_id);
        components_data.current_id += 1;
        //TODO: check for current_id overflow?
    }

    pub fn type_to_id<C: Replicate>() -> ComponentId {
        let type_id = TypeId::of::<C>();
        match COMPONENTS_DATA.lock() {
            Ok(components_data) => {
                return *components_data.type_to_id_map.get(&type_id).expect(
                    "Must properly initialize Component with Protocol via `add_component()` function!",
                );
            }
            Err(poison) => {
                panic!("type_to_id Error: {}", poison);
            }
        }
    }

    pub fn id_to_name(id: &ComponentId) -> String {
        todo!()
    }

    pub fn box_to_id(boxed_component: &Box<dyn Replicate>) -> ComponentId {
        todo!()
    }

    pub fn cast<R: Replicate>(boxed_component: Box<dyn Replicate>) -> Option<R> {
        todo!()
    }

    pub fn cast_ref<R: Replicate>(boxed_component: &Box<dyn Replicate>) -> Option<&R> {
        todo!()
    }

    pub fn cast_mut<R: Replicate>(
        boxed_component: &mut Box<dyn Replicate>,
    ) -> Option<&mut R> {
        todo!()
    }

    pub fn read(
        bit_reader: &mut BitReader,
        converter: &dyn NetEntityHandleConverter,
    ) -> Result<Box<dyn Replicate>, SerdeErr> {
        todo!()
    }

    pub fn read_create_update(bit_reader: &mut BitReader) -> Result<ComponentUpdate, SerdeErr> {
        todo!()
    }
}

lazy_static! {
    static ref COMPONENTS_DATA: Mutex<ComponentsData> = Mutex::new(ComponentsData::new());
}

struct ComponentsData {
    pub current_id: u16,
    pub type_to_id_map: HashMap<TypeId, ComponentId>,
}

impl ComponentsData {
    pub fn new() -> Self {
        Self {
            current_id: 0,
            type_to_id_map: HashMap::new(),
        }
    }
}

/// A struct that implements Replicate is a Component, or otherwise,
/// a container of Properties that can be scoped, tracked, and synced, with a
/// remote host
pub trait Replicate: ReplicateInner + Named {
    /// Gets the ComponentId of this type
    fn kind(&self) -> ComponentId;
    /// Gets the number of bytes of the Component's DiffMask
    fn diff_mask_size(&self) -> u8;
    /// Get an immutable reference to the inner Component as a Replicate trait object
    fn dyn_ref(&self) -> ReplicaDynRef<'_>;
    /// Get an mutable reference to the inner Component as a Replicate trait object
    fn dyn_mut(&mut self) -> ReplicaDynMut<'_>;
    /// Sets the current Replica to the state of another Replica of the
    /// same type
    fn mirror(&mut self, other: &dyn Replicate);
    /// Set the Component's PropertyMutator, which keeps track
    /// of which Properties have been mutated, necessary to sync only the
    /// Properties that have changed with the client
    fn set_mutator(&mut self, mutator: &PropertyMutator);
    /// Writes data into an outgoing byte stream, sufficient to completely
    /// recreate the Component on the client
    fn write(&self, bit_writer: &mut dyn BitWrite, converter: &dyn NetEntityHandleConverter);
    /// Write data into an outgoing byte stream, sufficient only to update the
    /// mutated Properties of the Component on the client
    fn write_update(
        &self,
        diff_mask: &DiffMask,
        bit_writer: &mut dyn BitWrite,
        converter: &dyn NetEntityHandleConverter,
    );
    /// Reads data from an incoming packet, sufficient to sync the in-memory
    /// Component with it's replica on the Server
    fn read_apply_update(
        &mut self,
        converter: &dyn NetEntityHandleConverter,
        update: ComponentUpdate,
    ) -> Result<(), SerdeErr>;
    /// Returns whether has any EntityProperties
    fn has_entity_properties(&self) -> bool;
    /// Returns a list of Entities contained within the Replica's properties
    fn entities(&self) -> Vec<EntityHandle>;
}

cfg_if! {
    if #[cfg(feature = "bevy_support")]
    {
        // Require that Bevy Component to be implemented
        use bevy_ecs::component::{TableStorage, Component};

        pub trait ReplicateInner: Component<Storage = TableStorage> + Sync + Send + 'static {}

        impl<T> ReplicateInner for T
        where T: Component<Storage = TableStorage> + Sync + Send + 'static {
        }
    }
    else
    {
        pub trait ReplicateInner: Sync + Send + 'static {}

        impl<T> ReplicateInner for T
        where T: Sync + Send + 'static {
        }
    }
}
