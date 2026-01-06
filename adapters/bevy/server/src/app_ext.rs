use bevy_app::App;

use naia_bevy_shared::{Replicate, ReplicateBundle};

use crate::{
    component_event_registry::ComponentEventRegistry,
    events::InsertBundleEvent,
    events::{InsertComponentEvent, RemoveComponentEvent, UpdateComponentEvent},
};

// App Extension Methods
pub trait AppRegisterComponentEvents {
    fn add_component_events<C: Replicate>(&mut self) -> &mut Self;
    fn add_bundle_events<B: ReplicateBundle>(&mut self) -> &mut Self;
}

impl AppRegisterComponentEvents for App {
    fn add_component_events<C: Replicate>(&mut self) -> &mut Self {
        // add component type to registry
        let mut component_event_registry =
            self.world_mut().resource_mut::<ComponentEventRegistry>();
        component_event_registry.register_component_handler::<C>();

        // add events
        self.add_event::<InsertComponentEvent<C>>()
            .add_event::<UpdateComponentEvent<C>>()
            .add_event::<RemoveComponentEvent<C>>();

        self
    }

    fn add_bundle_events<B: ReplicateBundle>(&mut self) -> &mut Self {
        // add component type to registry
        let mut component_event_registry =
            self.world_mut().resource_mut::<ComponentEventRegistry>();
        component_event_registry
            .bundle_registry_mut()
            .register_bundle_handler::<B>();

        // add events
        self.add_event::<InsertBundleEvent<B>>();

        self
    }
}
