use std::hash::Hash;
use std::marker::PhantomData;

use crate::{EntityEvent, GlobalWorldManagerType, WorldMutType};

pub struct SharedGlobalWorldManager<E: Copy + Eq + Hash + Send + Sync> {
    phantom_e: PhantomData<E>,
}

impl<E: Copy + Eq + Hash + Send + Sync> SharedGlobalWorldManager<E> {
    pub fn despawn_all_entities<W: WorldMutType<E>>(
        world: &mut W,
        global_world_manager: &dyn GlobalWorldManagerType<E>,
        entities: Vec<E>,
    ) -> Vec<EntityEvent<E>> {
        let mut output = Vec::new();

        for entity in entities {
            // Generate remove event for each component, handing references off just in
            // case
            if let Some(component_kinds) = global_world_manager.component_kinds(&entity) {
                for component_kind in component_kinds {
                    if let Some(component) =
                        world.remove_component_of_kind(&entity, &component_kind)
                    {
                        output.push(EntityEvent::<E>::RemoveComponent(entity, component));
                    } else {
                        panic!("Global World Manager must not have an accurate component list");
                    }
                }
            }

            // Generate despawn event
            output.push(EntityEvent::DespawnEntity(entity));

            // Despawn entity
            world.despawn_entity(&entity);
        }

        output
    }
}
