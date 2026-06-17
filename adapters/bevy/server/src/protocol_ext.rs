use std::sync::Arc;

use bevy_ecs::component::{Component, Mutable};

use naia_bevy_shared::{Protocol, Replicate};

use crate::app_ext::AppRegisterComponentEvents;

/// Extension on [`Protocol`] that registers a replicated component for wire
/// replication **and** bakes in a type-erased server event-installer closure
/// that will call `app.add_component_events::<C>()` when the
/// `NaiaServerPlugin::build` runs.
///
/// Unlike the client extension there is no `T` marker — the server
/// `ComponentEventRegistry` is not parameterised by a client tag.  `C` is the
/// only type captured into the closure, which is stored as a type-erased
/// `Arc<dyn Fn(&mut App) + Send + Sync>` on `Protocol` — no layering cycle
/// between `naia-bevy-shared` and `naia-bevy-server`.
///
/// The shared `Protocol::register_component` is the wire-only primitive.  This
/// trait is the tier-specific extension for the server side; client-side callers
/// use `ProtocolClientExt::add_component` (in `naia-bevy-client`).
///
/// # Usage
/// ```ignore
/// protocol.add_component::<NetworkedPosition>();
/// ```
pub trait ProtocolServerExt {
    fn add_component<C>(&mut self) -> &mut Self
    where
        C: Replicate + Component<Mutability = Mutable>;
}

impl ProtocolServerExt for Protocol {
    fn add_component<C>(&mut self) -> &mut Self
    where
        C: Replicate + Component<Mutability = Mutable>,
    {
        // Wire registration — same as `protocol.register_component::<C>()`.
        self.register_component::<C>();

        // Bake the server event installer into the protocol.  `C` is known at
        // this exact call site and captured by value (zero-sized marker), so
        // the Arc stores no heap data beyond the vtable.
        self.push_server_event_installer(Arc::new(|app| {
            app.add_component_events::<C>();
        }));

        self
    }
}
