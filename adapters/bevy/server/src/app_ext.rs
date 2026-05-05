use bevy_app::App;

use naia_bevy_shared::{Replicate, ReplicateBundle};

use crate::{
    component_event_registry::ComponentEventRegistry,
    events::InsertBundleEvent,
    events::{
        InsertComponentEvent, InsertResourceEvent, RemoveComponentEvent, RemoveResourceEvent,
        UpdateComponentEvent, UpdateResourceEvent,
    },
};

// App Extension Methods
pub trait AppRegisterComponentEvents {
    fn add_component_events<C: Replicate>(&mut self) -> &mut Self;
    fn add_bundle_events<B: ReplicateBundle>(&mut self) -> &mut Self;
    /// Register the user-facing event types for Replicated Resource `R`.
    /// Adds `InsertResourceEvent<R>`, `UpdateResourceEvent<R>`, and
    /// `RemoveResourceEvent<R>` as bevy `Message` types.
    ///
    /// Per D17 of `_AGENTS/RESOURCES_PLAN.md`: this method extends the
    /// existing `AppRegisterComponentEvents` trait rather than introducing
    /// a new trait — keeps user trait imports minimal.
    ///
    /// The shared `Protocol` must also register `R` via
    /// `protocol.add_resource::<R>()` in the user's `ProtocolPlugin`.
    fn add_resource_events<R: Replicate>(&mut self) -> &mut Self;
}

impl AppRegisterComponentEvents for App {
    fn add_component_events<C: Replicate>(&mut self) -> &mut Self {
        // add component type to registry
        let mut component_event_registry =
            self.world_mut().resource_mut::<ComponentEventRegistry>();
        component_event_registry.register_component_handler::<C>();

        // add events
        self.add_message::<InsertComponentEvent<C>>()
            .add_message::<UpdateComponentEvent<C>>()
            .add_message::<RemoveComponentEvent<C>>();

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
        self.add_message::<InsertBundleEvent<B>>();

        self
    }

    fn add_resource_events<R: Replicate>(&mut self) -> &mut Self {
        self.add_message::<InsertResourceEvent<R>>()
            .add_message::<UpdateResourceEvent<R>>()
            .add_message::<RemoveResourceEvent<R>>();

        self
    }
}
