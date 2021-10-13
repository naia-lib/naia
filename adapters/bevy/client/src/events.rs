use naia_client::{ProtocolType, OwnedEntity as NaiaOwnedEntity};

use naia_bevy_shared::Entity;

pub type OwnedEntity = NaiaOwnedEntity<Entity>;

pub struct SpawnEntityEvent<P: ProtocolType>(Entity, Vec<P>);
pub struct DespawnEntityEvent(Entity);
pub struct OwnEntity(OwnedEntity);
pub struct DisownEntity(OwnedEntity);
pub struct RewindEntity(OwnedEntity);
pub struct InsertComponent<P: ProtocolType>(Entity, P);
pub struct UpdateComponent<P: ProtocolType>(Entity, P);
pub struct RemoveComponent<P: ProtocolType>(Entity, P);
pub struct Message<P: ProtocolType>(P);
pub struct NewCommand<P: ProtocolType>(OwnedEntity, P);
pub struct ReplayCommand<P: ProtocolType>(OwnedEntity, P);