use std::{any::TypeId, collections::HashMap, marker::PhantomData, ops::Deref};

use hecs::World;

use naia_server::{ImplRef, EntityType, ProtocolType, Ref, Replicate, WorldType};

use super::{component_access::ComponentAccess, entity::Entity};

// WorldAdapt

pub trait WorldAdapt<'w> {
    fn adapt(self) -> WorldAdapter<'w>;
}

impl<'w> WorldAdapt<'w> for &'w mut World {
    fn adapt(self) -> WorldAdapter<'w> {
        return WorldAdapter::new(self);
    }
}

// WorldAdapter
pub struct WorldAdapter<'w> {
    world: &'w mut World,
}

impl<'w, P: ProtocolType> WorldType<P, Entity> for WorldAdapter<'w> {
    fn has_entity(&self, entity_key: &Entity) -> bool {
        return self.hecs.contains(entity_key.0);
    }

    fn entities(&self) -> Vec<Entity> {
        let mut output = Vec::new();

        for (entity, _) in self.hecs.iter() {
            output.push(Entity::new(entity));
        }

        return output;
    }

    fn spawn_entity(&mut self) -> Entity {
        let entity = self.hecs.spawn(());
        return Entity::new(entity);
    }

    fn despawn_entity(&mut self, entity_key: &Entity) {
        self.hecs
            .despawn(entity_key.0)
            .expect("error despawning Entity");
    }

    fn has_component<R: Replicate<P>>(&self, entity_key: &Entity) -> bool {
        let result = self.hecs.get::<Ref<R>>(entity_key.0);
        return result.is_ok();
    }

    fn has_component_of_type(&self, entity_key: &Entity, type_id: &TypeId) -> bool {
        return WorldType::<P, Entity>::get_component_from_type(self, entity_key, type_id)
            .is_some();
    }

    fn get_component<R: Replicate<P>>(&self, entity_key: &Entity) -> Option<Ref<R>> {
        return self
            .hecs
            .get::<Ref<R>>(entity_key.0)
            .map_or(None, |v| Some(v.deref().clone()));
    }

    fn get_component_from_type(&self, entity_key: &Entity, type_id: &TypeId) -> Option<P> {
        if let Some(handler) = self.rep_type_to_handler_map.get(type_id) {
            return handler.get_component(self, &entity_key.0);
        }
        return None;
    }

    fn get_components(&self, entity_key: &Entity) -> Vec<P> {
        let mut protocols = Vec::new();

        if let Ok(entity_ref) = self.hecs.entity(entity_key.0) {
            for ref_type in entity_ref.component_types() {
                if let Some(rep_type) = self.ref_type_to_rep_type_map.get(&ref_type) {
                    if let Some(component) = WorldType::<P, Entity>::get_component_from_type(
                        self, entity_key, &rep_type,
                    ) {
                        protocols.push(component);
                    }
                }
            }
        }

        return protocols;
    }

    fn insert_component<R: ImplRef<P>>(&mut self, entity_key: &Entity, component_ref: R) {
        // cache type id for later
        // todo: can we initialize this map on startup via Protocol derive?
        let inner_type_id = component_ref.dyn_ref().borrow().get_type_id();
        if !self.rep_type_to_handler_map.contains_key(&inner_type_id) {
            self.rep_type_to_handler_map
                .insert(inner_type_id, Handler::<P, R>::new());
            self.ref_type_to_rep_type_map
                .insert(TypeId::of::<R>(), inner_type_id);
        }

        // insert into ecs
        self.hecs
            .insert_one(entity_key.0, component_ref)
            .expect("error inserting Component");
    }

    fn remove_component<R: Replicate<P>>(&mut self, entity_key: &Entity) {
        // remove from ecs
        self.hecs
            .remove_one::<Ref<R>>(entity_key.0)
            .expect("error removing Component");
    }
}
