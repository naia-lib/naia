use naia_shared::{EntityType, ProtocolType, ProtocolKindType, Replicate};

use super::owned_entity::OwnedEntity;

/// An Event that is be emitted by the Client, usually as a result of some
/// communication with the Server
#[derive(Debug)]
pub enum Event<P: ProtocolType, E: EntityType> {
    /// Occurs when the Client has successfully established a connection with
    /// the Server
    Connection,
    /// Occurs when the Client has lost connection with the Server, usually as a
    /// result of a timeout
    Disconnection,
    /// A Tick Event, the duration between Tick events is defined in the Config
    /// passed to the Client on initialization
    Tick,
    /// Occurs when an Entity on the Server has come into scope for the Client
    SpawnEntity(E, Vec<P::Kind>),
    /// Occurs when an Entity on the Server has been destroyed, or left the
    /// Client's scope
    DespawnEntity(E),
    /// Occurs when an Entity has been assigned to the current User,
    /// meaning it can receive Commands from the Client, and be extrapolated
    /// forward into the "present" time
    OwnEntity(OwnedEntity<E>),
    /// Occurs when an Entity has been unassigned from the current User,
    /// meaning it can no longer receive Commands from this Client
    DisownEntity(OwnedEntity<E>),
    /// Occurs when an assigned Entity needs to be reset from the "present"
    /// state, back to it's authoritative "past", due to some misprediction
    /// error
    RewindEntity(OwnedEntity<E>),
    /// Occurs when a Component should be added to a given Entity
    InsertComponent(E, P::Kind),
    /// Occurs when a Component has had a state change on the Server while
    /// the Entity it is attached to has come into scope for the Client
    UpdateComponent(E, P::Kind),
    /// Occurs when a Component should be removed from the given Entity
    RemoveComponent(E, P::Kind),
    /// An Message emitted to the Client from the Server
    Message(P),
    /// A new Command received immediately to an assigned Entity, used to
    /// extrapolate Entity from the "past" to the "present".
    NewCommand(OwnedEntity<E>, P),
    /// An old Command which has already been received by the assigned Entity,
    /// but which must be replayed after a "RewindEntity" event in order
    /// to extrapolate back to the "present"
    ReplayCommand(OwnedEntity<E>, P),
}


