use bevy_ecs::component::Component;

use naia_bevy_shared::HostOwned;

/// Marker component present on entities owned (authoritative) by this client.
pub type ClientOwned = HostOwned;

/// Marker component inserted on every entity spawned here because the
/// server replicated it to this client.
#[derive(Component)]
pub struct ServerOwned;
