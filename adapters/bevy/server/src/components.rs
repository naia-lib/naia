use bevy_ecs::component::Component;

use naia_bevy_shared::HostOwned;
use naia_server::UserKey;

pub type ServerOwned = HostOwned;

#[derive(Component)]
pub struct ClientOwned(pub UserKey);

/// Marker component that drives entity replication (naia-lib/naia#182).
///
/// Inserting `Replication` on an entity begins replication; removing it (or
/// despawning the entity) ends replication. The `enable_replication` /
/// `disable_replication` commands converge onto this same marker, so the
/// command path and the marker path can never disagree about an entity.
/// This is deliberately not a `#[derive(Replicate)]` component: it is never
/// replicated itself, it only switches replication of its entity on and off.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Replication;
