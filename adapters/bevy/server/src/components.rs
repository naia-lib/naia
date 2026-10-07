use bevy_ecs::component::Component;

use naia_bevy_shared::HostOwned;
use naia_server::UserKey;

/// Server-side alias for [`HostOwned`].
pub type ServerOwned = HostOwned;

/// Component holding the [`UserKey`] of the client that owns this entity.
#[derive(Component)]
pub struct ClientOwned(pub UserKey);
