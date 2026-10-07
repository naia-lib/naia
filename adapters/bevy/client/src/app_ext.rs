use bevy_app::App;

use naia_bevy_shared::{Replicate, ReplicateBundle, ReplicatedResource};

use crate::{
    component_event_registry::ComponentEventRegistry,
    events::{
        InsertBundleEvent, InsertComponentEvent, InsertResourceEvent, RemoveComponentEvent,
        RemoveResourceEvent, UpdateComponentEvent, UpdateResourceEvent,
    },
};

/// Extension methods on Bevy's `App` for registering the lifecycle event
/// types (insert/update/remove, and bundle inserts) for a replicated
/// component or resource scoped under client-tag `T`.
pub trait AppRegisterComponentEvents {
    /// Registers `InsertComponentEvent<T, C>`, `UpdateComponentEvent<T, C>`,
    /// and `RemoveComponentEvent<T, C>` as bevy `Message` types, and records
    /// `C` in the client-tag `T` component-event registry so incoming
    /// component events route to them.
    fn add_component_events<T: Send + Sync + 'static, C: Replicate>(&mut self) -> &mut Self;
    /// Registers `InsertBundleEvent<T, B>` as a bevy `Message` type and
    /// records bundle `B` in the client-tag `T` registry, so the event
    /// fires once all of `B`'s components are present on an entity.
    fn add_bundle_events<T: Send + Sync + 'static, B: ReplicateBundle>(&mut self) -> &mut Self;
    /// Register the user-facing lifecycle event types for Replicated
    /// Resource `R` scoped under client-tag `T`: `InsertResourceEvent<T, R>`,
    /// `UpdateResourceEvent<T, R>`, and `RemoveResourceEvent<T, R>` as bevy
    /// `Message` types.
    ///
    /// The resource VALUE is delivered for free under bevy 0.19 storage
    /// aliasing: the replicated carrier entity-component IS `Res<R>` (a
    /// Component+Resource type shares one storage cell), so when the client
    /// receives the carrier component `Res<R>` is populated automatically,
    /// and per-field server updates flow through the standard component
    /// apply path. This method is OPTIONAL — only needed for the
    /// insert/update/remove lifecycle messages (and to suppress the
    /// equivalent component-events; see `add_component_events`).
    ///
    /// The shared `Protocol` must register `R` via
    /// `protocol.add_resource::<R>()` in the user's `ProtocolPlugin`.
    fn add_resource_events<T, R>(&mut self) -> &mut Self
    where
        T: Send + Sync + 'static,
        R: ReplicatedResource;
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

    fn add_resource_events<T, R>(&mut self) -> &mut Self
    where
        T: Send + Sync + 'static,
        R: ReplicatedResource,
    {
        self.add_message::<InsertResourceEvent<T, R>>()
            .add_message::<UpdateResourceEvent<T, R>>()
            .add_message::<RemoveResourceEvent<T, R>>();

        self
    }
}
