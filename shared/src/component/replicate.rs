use std::any::Any;
use std::hash::Hash;
use std::sync::MutexGuard;
use std::{any::TypeId, collections::HashMap, sync::Mutex};

use lazy_static::lazy_static;

use naia_serde::{BitReader, BitWrite, Serde, SerdeErr};

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
    WorldMutType,
};

/// A map to hold all component types
pub struct Components;

impl Components {
    pub fn add_component<C: Replicate>() {
        let type_id = TypeId::of::<C>();
        let builder = C::create_builder();
        Self::get_data().add_component(&type_id, builder);
    }

    pub fn type_to_id<C: Replicate>() -> ComponentId {
        let type_id = TypeId::of::<C>();
        return Self::get_data().get_id(&type_id);
    }

    pub fn read(
        reader: &mut BitReader,
        converter: &dyn NetEntityHandleConverter,
    ) -> Result<Box<dyn Replicate>, SerdeErr> {
        let component_id: ComponentId = ComponentId::de(reader)?;
        return Self::get_data()
            .get_builder(&component_id)
            .read(reader, converter);
    }

    pub fn read_create_update(reader: &mut BitReader) -> Result<ComponentUpdate, SerdeErr> {
        todo!()
    }

    pub fn cast_ref<R: Replicate>(boxed_component: &Box<dyn Replicate>) -> Option<&R> {
        boxed_component.to_any().downcast_ref::<R>()
    }

    pub fn cast_mut<R: Replicate>(boxed_component: &mut Box<dyn Replicate>) -> Option<&mut R> {
        boxed_component.to_any_mut().downcast_mut::<R>()
    }

    fn get_data() -> MutexGuard<'static, ComponentsData> {
        match COMPONENTS_DATA.lock() {
            Ok(components_data) => {
                return components_data;
            }
            Err(poison) => {
                panic!("Components::get_data() Error: {}", poison);
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
}

lazy_static! {
    static ref COMPONENTS_DATA: Mutex<ComponentsData> = Mutex::new(ComponentsData::new());
}

struct ComponentsData {
    current_id: u16,
    type_to_id_map: HashMap<TypeId, ComponentId>,
    id_to_data_map: HashMap<ComponentId, Box<dyn ReplicateBuilder>>,
}

impl ComponentsData {
    fn new() -> Self {
        Self {
            current_id: 0,
            type_to_id_map: HashMap::new(),
            id_to_data_map: HashMap::new(),
        }
    }

    fn add_component(&mut self, type_id: &TypeId, builder: Box<dyn ReplicateBuilder>) {
        let component_id = ComponentId::new(self.current_id);
        self.type_to_id_map.insert(*type_id, component_id);
        self.id_to_data_map.insert(component_id, builder);
        self.current_id += 1;
        //TODO: check for current_id overflow?
    }

    fn get_id(&self, type_id: &TypeId) -> ComponentId {
        return *self.type_to_id_map.get(type_id).expect(
            "Must properly initialize Component with Protocol via `add_component()` function!",
        );
    }

    fn get_builder(&self, id: &ComponentId) -> &Box<dyn ReplicateBuilder> {
        return self.id_to_data_map.get(&id).expect(
            "Must properly initialize Component with Protocol via `add_component()` function!",
        );
    }
}

pub trait ReplicateBuilder: Send {
    /// Create new Component from incoming bit stream
    fn read(
        &self,
        reader: &mut BitReader,
        converter: &dyn NetEntityHandleConverter,
    ) -> Result<Box<dyn Replicate>, SerdeErr>;
    /// Create new Component Update from incoming bit stream
    fn read_create_update(&self, reader: &mut BitReader) -> Result<ComponentUpdate, SerdeErr>;
}

/// A struct that implements Replicate is a Component, or otherwise,
/// a container of Properties that can be scoped, tracked, and synced, with a
/// remote host
pub trait Replicate: ReplicateInner + Named + Any {
    fn to_any(&self) -> &dyn Any;
    fn to_any_mut(&mut self) -> &mut dyn Any;
    fn create_builder() -> Box<dyn ReplicateBuilder>
    where
        Self: Sized;
    fn copy_to_box(&self) -> Box<dyn Replicate>;
    /// Gets the ComponentId of this type
    fn kind(&self) -> ComponentId;
    /// Gets the number of bytes of the Component's DiffMask
    fn diff_mask_size(&self) -> u8;
    /// Get an immutable reference to the inner Component as a Replicate trait object
    fn dyn_ref(&self) -> ReplicaDynRef<'_>;
    /// Get an mutable reference to the inner Component as a Replicate trait object
    fn dyn_mut(&mut self) -> ReplicaDynMut<'_>;
    /// Sets the current Component to the state of another Component of the
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
