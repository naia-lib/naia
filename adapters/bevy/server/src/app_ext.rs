use bevy_app::App;
use bevy_ecs::component::Mutable;

use naia_bevy_shared::{HostComponent, Replicate, ReplicateBundle};

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
    /// Register the user-facing lifecycle event types for Replicated
    /// Resource `R`: `InsertResourceEvent<R>`, `UpdateResourceEvent<R>`,
    /// and `RemoveResourceEvent<R>` as bevy `Message` types.
    ///
    /// Replication of the resource VALUE happens for free under bevy 0.19
    /// storage aliasing: a `ReplicatedResource` is both a bevy `Component`
    /// (the host-owned carrier entity-component, with the native naia
    /// `PropertyMutator` attached at insert) and a bevy `Resource`
    /// (`Res<R>`), and they share one storage cell — so `ResMut<R>` writes
    /// are diff-tracked and replicate via the standard component path with
    /// no mirror. This method is OPTIONAL and only needed if the consumer
    /// wants the insert/update/remove lifecycle messages (and to suppress
    /// the equivalent component-events; see `add_component_events`).
    ///
    /// The shared `Protocol` must register `R` via
    /// `protocol.add_resource::<R>()` in the user's `ProtocolPlugin`, and
    /// `commands.replicate_resource::<R>(..)` inserts the value.
    fn add_resource_events<R>(&mut self) -> &mut Self
    where
        R: HostComponent
            + Replicate
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
        R: HostComponent
            + Replicate
            + bevy_ecs::resource::Resource
            + bevy_ecs::component::Component<Mutability = Mutable>,
    {
        // Register the user-facing lifecycle event types as bevy messages.
        // Value replication is handled by aliasing + the carrier
        // component's native naia mutator — no mirror system to install.
        self.add_message::<InsertResourceEvent<R>>()
            .add_message::<UpdateResourceEvent<R>>()
            .add_message::<RemoveResourceEvent<R>>();

        self
    }
}
