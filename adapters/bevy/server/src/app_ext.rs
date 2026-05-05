use bevy_app::App;
use bevy_ecs::component::Mutable;

use naia_bevy_shared::{Replicate, ReplicateBundle};

use crate::{
    component_event_registry::ComponentEventRegistry,
    events::InsertBundleEvent,
    events::{
        InsertComponentEvent, InsertResourceEvent, RemoveComponentEvent, RemoveResourceEvent,
        UpdateComponentEvent, UpdateResourceEvent,
    },
    resource_sync::{install_resource_sync_system, SyncDirtyTracker},
};

// App Extension Methods
pub trait AppRegisterComponentEvents {
    fn add_component_events<C: Replicate>(&mut self) -> &mut Self;
    fn add_bundle_events<B: ReplicateBundle>(&mut self) -> &mut Self;
    /// Register the user-facing event types for Replicated Resource `R`.
    /// Adds `InsertResourceEvent<R>`, `UpdateResourceEvent<R>`, and
    /// `RemoveResourceEvent<R>` as bevy `Message` types.
    ///
    /// Also installs the **Mode B mirror system** for `R`: a per-tick
    /// system that drains the bevy-resource side's `SyncDirtyTracker<R>`
    /// and propagates each touched `Property<T>` field to the entity-
    /// component side via `Replicate::mirror_single_field`. The result:
    /// users access `R` via standard `Res<R>` / `ResMut<R>`, mutations
    /// replicate per-field with no over-replication.
    ///
    /// Per D17 of `_AGENTS/RESOURCES_PLAN.md`: this method extends the
    /// existing `AppRegisterComponentEvents` trait rather than introducing
    /// a new trait — keeps user trait imports minimal.
    ///
    /// The shared `Protocol` must also register `R` via
    /// `protocol.add_resource::<R>()` in the user's `ProtocolPlugin`.
    fn add_resource_events<R>(&mut self) -> &mut Self
    where
        R: Replicate
            + bevy_ecs::resource::Resource
            + bevy_ecs::component::Component<Mutability = Mutable>;
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

    fn add_resource_events<R>(&mut self) -> &mut Self
    where
        R: Replicate
            + bevy_ecs::resource::Resource
            + bevy_ecs::component::Component<Mutability = Mutable>,
    {
        // Register the user-facing event types as bevy messages.
        self.add_message::<InsertResourceEvent<R>>()
            .add_message::<UpdateResourceEvent<R>>()
            .add_message::<RemoveResourceEvent<R>>();

        // Install Mode B mirror infrastructure: tracker + sync system.
        // Idempotent — re-registration is a no-op.
        if self.world().get_resource::<SyncDirtyTracker<R>>().is_none() {
            self.insert_resource(SyncDirtyTracker::<R>::default());
        }
        install_resource_sync_system::<R>(self);

        self
    }
}
