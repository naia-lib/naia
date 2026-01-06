use std::{any::Any, collections::HashMap, marker::PhantomData};

use bevy_ecs::{entity::Entity, resource::Resource, world::World};

use log::warn;

use naia_bevy_shared::{ComponentKind, Replicate};

use naia_server::UserKey;

use crate::{
    bundle_event_registry::BundleEventRegistry,
    events::{InsertComponentEvent, RemoveComponentEvent, UpdateComponentEvent},
};

#[derive(Resource)]
pub(crate) struct ComponentEventRegistry {
    bundle_registry: BundleEventRegistry,
    component_handlers: HashMap<ComponentKind, Box<dyn ComponentEventHandler>>,
}

unsafe impl Send for ComponentEventRegistry {}
unsafe impl Sync for ComponentEventRegistry {}

impl Default for ComponentEventRegistry {
    fn default() -> Self {
        Self {
            component_handlers: HashMap::new(),
            bundle_registry: BundleEventRegistry::default(),
        }
    }
}

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
            world.send_event(InsertComponentEvent::<R>::new(user_key, entity));
        }
    }

    fn handle_updates(&mut self, world: &mut World, entities: Vec<(UserKey, Entity)>) {
        for (user_key, entity) in entities {
            world.send_event(UpdateComponentEvent::<R>::new(user_key, entity));
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
            world.send_event(RemoveComponentEvent::<R>::new(user_key, entity, component));
        }
    }
}
