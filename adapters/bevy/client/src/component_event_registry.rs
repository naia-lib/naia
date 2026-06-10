use std::{any::Any, collections::HashMap, marker::PhantomData};

use bevy_ecs::{entity::Entity, message::Messages, resource::Resource, world::World};

use log::trace;

use naia_bevy_shared::{ComponentKind, Replicate, Tick};

use crate::{
    bundle_event_registry::BundleEventRegistry,
    events::{
        InsertComponentEvent, InsertResourceEvent, RemoveComponentEvent, RemoveResourceEvent,
        UpdateComponentEvent, UpdateResourceEvent,
    },
};

#[derive(Resource)]
pub(crate) struct ComponentEventRegistry<T: Send + Sync + 'static> {
    bundle_registry: BundleEventRegistry<T>,
    component_handlers: HashMap<ComponentKind, Box<dyn ComponentEventHandler>>,
}

impl<T: Send + Sync + 'static> Default for ComponentEventRegistry<T> {
    fn default() -> Self {
        Self {
            component_handlers: HashMap::new(),
            bundle_registry: BundleEventRegistry::default(),
        }
    }
}

impl<T: Send + Sync + 'static> ComponentEventRegistry<T> {
    pub(crate) fn bundle_registry_mut(&mut self) -> &mut BundleEventRegistry<T> {
        &mut self.bundle_registry
    }

    pub(crate) fn register_component_handler<R: Replicate>(&mut self) {
        self.component_handlers.insert(
            ComponentKind::of::<R>(),
            ComponentEventHandlerImpl::<T, R>::new_boxed(),
        );
    }

    pub(crate) fn receive_events(
        &mut self,
        world: &mut World,
        events: &mut naia_client::Events<Entity>,
    ) {
        // Insert Component Event
        if events.has_inserts() {
            let inserts = events.take_inserts().unwrap();

            self.bundle_registry.pre_process();

            for (kind, entities) in inserts {
                // trigger bundle events
                self.bundle_registry
                    .process_inserts(world, &kind, &entities);

                // trigger component events
                if let Some(component_handler) = self.component_handlers.get_mut(&kind) {
                    component_handler.handle_inserts(world, entities);
                } else {
                    // No event-based handler registered for this kind. This is
                    // routine, NOT an error: a component can be replicated and
                    // materialized onto the entity (read later by Query) without
                    // an insert-event system — e.g. deterministic seed state read
                    // by the sim, or components only some app modes handle. naia
                    // still inserts the component; only the optional event is
                    // absent. trace, don't warn, so the default console stays quiet.
                    trace!("No insert event handler for ComponentKind: {:?}", kind);
                }
            }
        }

        // Update Component Event
        if events.has_updates() {
            let updates = events.take_updates().unwrap();
            for (kind, entities) in updates {
                let Some(handler) = self.component_handlers.get_mut(&kind) else {
                    trace!("No update event handler for ComponentKind: {:?}", kind);
                    continue;
                };
                handler.handle_updates(world, entities);
            }
        }

        // Remove Component Event
        if events.has_removes() {
            let removes = events.take_removes().unwrap();
            for (kind, entities) in removes {
                let Some(handler) = self.component_handlers.get_mut(&kind) else {
                    trace!("No remove event handler for ComponentKind: {:?}", kind);
                    continue;
                };
                handler.handle_removes(world, entities);
            }
        }
    }
}

trait ComponentEventHandler: Send + Sync {
    fn handle_inserts(&mut self, world: &mut World, entities: Vec<Entity>);
    fn handle_updates(&mut self, world: &mut World, entities: Vec<(Tick, Entity)>);
    fn handle_removes(&mut self, world: &mut World, entities: Vec<(Entity, Box<dyn Replicate>)>);
}

struct ComponentEventHandlerImpl<T: Send + Sync + 'static, R: Replicate> {
    phantom_t: PhantomData<T>,
    phantom_r: PhantomData<R>,
}

impl<T: Send + Sync + 'static, R: Replicate> ComponentEventHandlerImpl<T, R> {
    fn new() -> Self {
        Self {
            phantom_t: PhantomData::<T>,
            phantom_r: PhantomData::<R>,
        }
    }

    fn new_boxed() -> Box<dyn ComponentEventHandler> {
        Box::new(Self::new())
    }
}

impl<T: Send + Sync + 'static, R: Replicate> ComponentEventHandler
    for ComponentEventHandlerImpl<T, R>
{
    fn handle_inserts(&mut self, world: &mut World, entities: Vec<Entity>) {
        for entity in entities {
            // D13 resource translation: if user registered
            // InsertResourceEvent<T, R> via add_resource_events, route
            // to that stream instead of the component-event stream.
            // Users see ZERO component-level semantics for resources.
            if world.contains_resource::<Messages<InsertResourceEvent<T, R>>>() {
                world
                    .resource_mut::<Messages<InsertResourceEvent<T, R>>>()
                    .write(InsertResourceEvent::<T, R>::new());
                continue;
            }
            world
                .resource_mut::<Messages<InsertComponentEvent<T, R>>>()
                .write(InsertComponentEvent::<T, R>::new(entity));
        }
    }

    fn handle_updates(&mut self, world: &mut World, entities: Vec<(Tick, Entity)>) {
        for (tick, entity) in entities {
            if world.contains_resource::<Messages<UpdateResourceEvent<T, R>>>() {
                world
                    .resource_mut::<Messages<UpdateResourceEvent<T, R>>>()
                    .write(UpdateResourceEvent::<T, R>::new(tick));
                continue;
            }
            world
                .resource_mut::<Messages<UpdateComponentEvent<T, R>>>()
                .write(UpdateComponentEvent::<T, R>::new(tick, entity));
        }
    }

    fn handle_removes(&mut self, world: &mut World, entities: Vec<(Entity, Box<dyn Replicate>)>) {
        for (entity, boxed_component) in entities {
            let boxed_any = boxed_component.copy_to_box().to_boxed_any();
            let component: R = Box::<dyn Any + 'static>::downcast::<R>(boxed_any)
                .ok()
                .map(|boxed_r| *boxed_r)
                .unwrap();
            if world.contains_resource::<Messages<RemoveResourceEvent<T, R>>>() {
                world
                    .resource_mut::<Messages<RemoveResourceEvent<T, R>>>()
                    .write(RemoveResourceEvent::<T, R>::new(component));
                continue;
            }
            world
                .resource_mut::<Messages<RemoveComponentEvent<T, R>>>()
                .write(RemoveComponentEvent::<T, R>::new(entity, component));
        }
    }
}
