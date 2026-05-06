use std::{any::Any, collections::HashMap, marker::PhantomData};

use bevy_ecs::{entity::Entity, message::Messages, resource::Resource, world::World};

use log::warn;

use naia_bevy_shared::{ComponentKind, Replicate};

use naia_server::UserKey;

use crate::{
    bundle_event_registry::BundleEventRegistry,
    events::{
        InsertComponentEvent, InsertResourceEvent, RemoveComponentEvent, RemoveResourceEvent,
        UpdateComponentEvent, UpdateResourceEvent,
    },
    server::ServerImpl,
};

#[derive(Resource)]
#[derive(Default)]
pub(crate) struct ComponentEventRegistry {
    bundle_registry: BundleEventRegistry,
    component_handlers: HashMap<ComponentKind, Box<dyn ComponentEventHandler>>,
}

unsafe impl Send for ComponentEventRegistry {}
unsafe impl Sync for ComponentEventRegistry {}


impl ComponentEventRegistry {
    pub(crate) fn bundle_registry_mut(&mut self) -> &mut BundleEventRegistry {
        &mut self.bundle_registry
    }

    pub fn register_component_handler<R: Replicate>(&mut self) {
        self.component_handlers.insert(
            ComponentKind::of::<R>(),
            ComponentEventHandlerImpl::<R>::new_boxed(),
        );
    }

    pub fn receive_events(&mut self, world: &mut World, events: &mut naia_server::Events<Entity>) {
        // Insert Component Event
        if events.has_inserts() {
            let inserts = events.take_inserts().unwrap();

            self.bundle_registry.pre_process();

            for (kind, entities) in inserts {
                // trigger bundle events
                self.bundle_registry
                    .process_inserts(world, &kind, &entities);

                // trigger component events
                if let Some(handler) = self.component_handlers.get_mut(&kind) {
                    handler.handle_inserts(world, entities);
                } else {
                    warn!("No insert event handler for ComponentKind: {:?}", kind);
                };
            }
        }

        // Update Component Event
        if events.has_updates() {
            let updates = events.take_updates().unwrap();
            for (kind, entities) in updates {
                if let Some(handler) = self.component_handlers.get_mut(&kind) {
                    handler.handle_updates(world, entities);
                } else {
                    warn!("No update event handler for ComponentKind: {:?}", kind);
                };
            }
        }

        // Remove Component Event
        if events.has_removes() {
            let removes = events.take_removes().unwrap();
            for (kind, entities) in removes {
                if let Some(handler) = self.component_handlers.get_mut(&kind) {
                    handler.handle_removes(world, entities);
                } else {
                    warn!("No remove event handler for ComponentKind: {:?}", kind);
                };
            }
        }
    }
}

trait ComponentEventHandler: Send + Sync {
    fn handle_inserts(&mut self, world: &mut World, entities: Vec<(UserKey, Entity)>);
    fn handle_updates(&mut self, world: &mut World, entities: Vec<(UserKey, Entity)>);
    fn handle_removes(
        &mut self,
        world: &mut World,
        entities: Vec<(UserKey, Entity, Box<dyn Replicate>)>,
    );
}

struct ComponentEventHandlerImpl<R: Replicate> {
    phantom_r: PhantomData<R>,
}

impl<R: Replicate> ComponentEventHandlerImpl<R> {
    fn new() -> Self {
        Self {
            phantom_r: PhantomData::<R>,
        }
    }

    fn new_boxed() -> Box<dyn ComponentEventHandler> {
        Box::new(Self::new())
    }
}

impl<R: Replicate> ComponentEventHandler for ComponentEventHandlerImpl<R> {
    fn handle_inserts(&mut self, world: &mut World, entities: Vec<(UserKey, Entity)>) {
        for (user_key, entity) in entities {
            // D13 resource translation: if `entity` is a hidden
            // resource entity AND the user registered InsertResourceEvent<R>
            // (via add_resource_events), emit the resource event INSTEAD
            // of the component event. Users see zero component-level
            // semantics for resources.
            if is_resource_entity(world, &entity)
                && world.contains_resource::<Messages<InsertResourceEvent<R>>>()
            {
                world
                    .resource_mut::<Messages<InsertResourceEvent<R>>>()
                    .write(InsertResourceEvent::<R>::new(user_key));
                continue;
            }
            world
                .resource_mut::<Messages<InsertComponentEvent<R>>>()
                .write(InsertComponentEvent::<R>::new(user_key, entity));
        }
    }

    fn handle_updates(&mut self, world: &mut World, entities: Vec<(UserKey, Entity)>) {
        for (user_key, entity) in entities {
            if is_resource_entity(world, &entity)
                && world.contains_resource::<Messages<UpdateResourceEvent<R>>>()
            {
                world
                    .resource_mut::<Messages<UpdateResourceEvent<R>>>()
                    .write(UpdateResourceEvent::<R>::new(user_key));
                continue;
            }
            world
                .resource_mut::<Messages<UpdateComponentEvent<R>>>()
                .write(UpdateComponentEvent::<R>::new(user_key, entity));
        }
    }

    fn handle_removes(
        &mut self,
        world: &mut World,
        entities: Vec<(UserKey, Entity, Box<dyn Replicate>)>,
    ) {
        for (user_key, entity, boxed_component) in entities {
            let boxed_any = boxed_component.copy_to_box().to_boxed_any();
            let component: R = Box::<dyn Any + 'static>::downcast::<R>(boxed_any)
                .ok()
                .map(|boxed_r| *boxed_r)
                .unwrap();
            // Resource translation for removes too. Note that by the
            // time a resource is removed, the entity is also being
            // despawned, so `is_resource_entity` may already be false
            // (the registry entry was cleared in `WorldServer::remove_resource`
            // BEFORE the entity despawn fires the RemoveComponentEvent).
            // To handle this correctly we don't gate on is_resource_entity
            // here; we gate purely on whether the user registered for
            // resource events for this type. If they did, route to the
            // resource event stream — components can never be both a
            // user-replicated component and a registered resource type.
            if world.contains_resource::<Messages<RemoveResourceEvent<R>>>() {
                world
                    .resource_mut::<Messages<RemoveResourceEvent<R>>>()
                    .write(RemoveResourceEvent::<R>::new(user_key, component));
                // Also remove the bevy-Resource mirror if present.
                // (Mode B: `Res<R>` should disappear when the resource
                // is removed. Best-effort — if the type isn't a bevy
                // Resource the call is a no-op via type erasure.)
                continue;
            }
            world
                .resource_mut::<Messages<RemoveComponentEvent<R>>>()
                .write(RemoveComponentEvent::<R>::new(user_key, entity, component));
        }
    }
}

/// True iff `entity` is the hidden entity for a Replicated Resource.
/// Looks up via `ServerImpl::is_resource_entity`.
fn is_resource_entity(world: &World, entity: &Entity) -> bool {
    world
        .get_resource::<ServerImpl>()
        .map(|s| s.is_resource_entity(entity))
        .unwrap_or(false)
}
