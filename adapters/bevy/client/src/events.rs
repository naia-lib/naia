use bevy::ecs::entity::Entity;

use naia_client::shared::Protocolize;

pub struct SpawnEntityEvent<P: Protocolize>(pub Entity, pub Vec<P::Kind>);
pub struct DespawnEntityEvent(pub Entity);
pub struct InsertComponentEvent<P: Protocolize>(pub Entity, pub P::Kind);
pub struct UpdateComponentEvent<P: Protocolize>(pub Entity, pub P::Kind);
pub struct RemoveComponentEvent<P: Protocolize>(pub Entity, pub P);
pub struct MessageEvent<P: Protocolize>(pub P);
