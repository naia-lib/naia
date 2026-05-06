use bevy_app::App;

use naia_bevy_shared::{Replicate, ReplicateBundle, ReplicatedResource};

use crate::{
    component_event_registry::ComponentEventRegistry,
    events::{
        InsertBundleEvent, InsertComponentEvent, InsertResourceEvent, RemoveComponentEvent,
        RemoveResourceEvent, UpdateComponentEvent, UpdateResourceEvent,
    },
    resource_sync::install_resource_sync_system,
};

// App Extension Methods
pub trait AppRegisterComponentEvents {
    fn add_component_events<T: Send + Sync + 'static, C: Replicate>(&mut self) -> &mut Self;
    fn add_bundle_events<T: Send + Sync + 'static, B: ReplicateBundle>(&mut self) -> &mut Self;
    /// Register the user-facing event types for Replicated Resource `R`
    /// scoped under client-tag `T`. Adds `InsertResourceEvent<T, R>`,
    /// `UpdateResourceEvent<T, R>`, and `RemoveResourceEvent<T, R>` as
    /// bevy `Message` types AND installs the Mode B mirror system
    /// (incoming entity-component → bevy `Resource<R>` + outgoing
    /// `ResMut<R>` → entity-component when client holds authority).
    ///
    /// Per D17 of `_AGENTS/RESOURCES_PLAN.md`: this method extends the
    /// existing `AppRegisterComponentEvents` trait — no new trait
    /// import for users to manage.
    ///
    /// The shared `Protocol` must also register `R` via
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

        // Install Mode B mirror infrastructure: per-tick dispatcher
        // running incoming (entity-component → bevy Resource) and
        // outgoing (bevy ResMut → entity-component → wire) sync
        // hooks. Idempotent.
        install_resource_sync_system::<T, R>(self);

        self
    }
}
