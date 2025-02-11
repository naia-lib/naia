use std::hash::Hash;

use crate::{EntityAndGlobalEntityConverter, EntityEvent, GlobalEntity, GlobalWorldManagerType, WorldMutType};

pub struct SharedGlobalWorldManager;

impl SharedGlobalWorldManager {
    pub fn despawn_all_entities<E: Copy + Eq + Hash + Send + Sync, W: WorldMutType<E>>(
        world: &mut W,
        converter: &dyn EntityAndGlobalEntityConverter<E>,
        global_world_manager: &dyn GlobalWorldManagerType,
        entities: Vec<GlobalEntity>,
    ) -> Vec<EntityEvent> {
        let mut output = Vec::new();

        for global_entity in entities {

            // Get world entity
            let world_entity = converter.global_entity_to_entity(&global_entity).unwrap();

            // Generate remove event for each component, handing references off just in
            // case
            if let Some(component_kinds) = global_world_manager.component_kinds(&global_entity) {
                for component_kind in component_kinds {
                    if let Some(component) =
                        world.remove_component_of_kind(&world_entity, &component_kind)
                    {
                        output.push(EntityEvent::RemoveComponent(global_entity, component));
                    } else {
                        panic!("Global World Manager must not have an accurate component list");
                    }
                }
            }

            // Generate despawn event
            output.push(EntityEvent::DespawnEntity(global_entity));

            // Despawn entity
            world.despawn_entity(&world_entity);
        }

        output
    }
}
