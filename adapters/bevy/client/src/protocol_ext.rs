use std::sync::Arc;

use bevy_ecs::component::{Component, Mutable};

use naia_bevy_shared::{Protocol, Replicate, ReplicateBundle};

use crate::app_ext::AppRegisterComponentEvents;

/// Extension on [`Protocol`] that registers a replicated component for wire
/// replication **and** bakes in a type-erased client event-installer closure
/// that will call `app.add_component_events::<T, C>()` when the
/// `NaiaClientPlugin::<T>` builds its `App`.
///
/// Both `T` (the naia client marker, known at plugin-build time) and `C` (the
/// component type, known at protocol-build time) are concrete at the call site
/// here, so they can be captured in a `move` closure without any `dyn` generic
/// method.  The closure itself is stored as a type-erased
/// `Arc<dyn Fn(&mut App) + Send + Sync>` on `Protocol`, which names no client
/// type — no `naia-bevy-shared → naia-bevy-client` layering cycle.
///
/// The shared `Protocol::register_component` is the wire-only primitive; this
/// trait is the tier-specific extension for the client side.  Server-side
/// callers use `ProtocolServerExt::add_component` (in `naia-bevy-server`).
///
/// `add_bundle` bakes in the bundle event installer (no wire registration —
/// a bundle is a tuple of already-registered components; only the bevy event
/// channels need setting up).  Server-side callers use
/// `ProtocolServerExt::add_bundle`.
///
/// # Usage
/// ```ignore
/// // in a protocol-builder function called with a concrete T:
/// protocol.add_component::<Game, NetworkedPosition>();
/// protocol.add_bundle::<Game, (AvatarUnit, NetworkedTileTarget)>();
/// ```
pub trait ProtocolClientExt {
    fn add_component<T, C>(&mut self) -> &mut Self
    where
        T: Send + Sync + 'static,
        C: Replicate + Component<Mutability = Mutable>;

    fn add_bundle<T, B>(&mut self) -> &mut Self
    where
        T: Send + Sync + 'static,
        B: ReplicateBundle;
}

impl ProtocolClientExt for Protocol {
    fn add_component<T, C>(&mut self) -> &mut Self
    where
        T: Send + Sync + 'static,
        C: Replicate + Component<Mutability = Mutable>,
    {
        // Wire registration — same as `protocol.register_component::<C>()`.
        self.register_component::<C>();

        // Bake the client event installer into the protocol.  `T` and `C` are
        // both known at this exact call site and captured by value (zero-sized
        // PhantomData), so the Arc stores no heap data beyond the vtable.
        self.push_client_event_installer(Arc::new(|app| {
            app.add_component_events::<T, C>();
        }));

        self
    }

    fn add_bundle<T, B>(&mut self) -> &mut Self
    where
        T: Send + Sync + 'static,
        B: ReplicateBundle,
    {
        // Bundles are tuples of already-registered components — no wire
        // registration needed.  Only the bevy event channels need setting up,
        // which `add_bundle_events::<T, B>()` does.
        self.push_client_event_installer(Arc::new(|app| {
            app.add_bundle_events::<T, B>();
        }));

        self
    }
}
