use bevy_app::App;

use naia_bevy_shared::{Replicate, ReplicateBundle};

use crate::{
    component_event_registry::ComponentEventRegistry,
    events::{InsertBundleEvent, InsertComponentEvent, RemoveComponentEvent, UpdateComponentEvent},
};

// App Extension Methods
pub trait AppRegisterComponentEvents {
    fn add_component_events<T: Send + Sync + 'static, C: Replicate>(&mut self) -> &mut Self;
    fn add_bundle_events<T: Send + Sync + 'static, B: ReplicateBundle>(&mut self) -> &mut Self;
}

impl AppRegisterComponentEvents for App {
    fn add_component_events<T: Send + Sync + 'static, C: Replicate>(&mut self) -> &mut Self {
        // add component type to registry
        let mut component_event_registry =
            self.world_mut().resource_mut::<ComponentEventRegistry<T>>();
        component_event_registry.register_component_handler::<C>();

        // add events
        self.add_message::<InsertComponentEvent<T, C>>()
            .add_message::<UpdateComponentEvent<T, C>>()
            .add_message::<RemoveComponentEvent<T, C>>();

        self
    }

    fn add_bundle_events<T: Send + Sync + 'static, B: ReplicateBundle>(&mut self) -> &mut Self {
        // add component type to registry
        let mut component_event_registry =
            self.world_mut().resource_mut::<ComponentEventRegistry<T>>();
        component_event_registry
            .bundle_registry_mut()
            .register_bundle_handler::<B>();

        // add events
        self.add_message::<InsertBundleEvent<T, B>>();

        self
    }
}
