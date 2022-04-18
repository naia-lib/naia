use bevy_ecs::entity::Entity;

use naia_client::shared::{ChannelIndex, ProtocolKindType, Protocolize, Tick};

pub struct SpawnEntityEvent(pub Entity);
pub struct DespawnEntityEvent(pub Entity);
pub struct InsertComponentEvent<K: ProtocolKindType>(pub Entity, pub K);
pub struct UpdateComponentEvent<K: ProtocolKindType>(pub Tick, pub Entity, pub K);
pub struct RemoveComponentEvent<P: Protocolize>(pub Entity, pub P);
pub struct MessageEvent<P: Protocolize, C: ChannelIndex>(pub C, pub P);
