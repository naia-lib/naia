use naia_shared::{EntityType, ProtocolType};

/// An Event that is be emitted by the Client, usually as a result of some
/// communication with the Server
#[derive(Debug)]
pub enum Event<P: ProtocolType, K: EntityType> {
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
    SpawnEntity(K, Vec<P>),
    /// Occurs when an Entity on the Server has been destroyed, or left the
    /// Client's scope
    DespawnEntity(K),
    /// Occurs when an Entity has been assigned to the current User,
    /// meaning it can receive Commands from the Client, and be extrapolated
    /// into the "present"
    OwnEntity(K),
    /// Occurs when an Entity has been unassigned from the current User,
    /// meaning it can no longer receive Commands from this Client
    DisownEntity(K),
    /// Occurs when an assigned Entity needs to be reset from the "present"
    /// state, back to it's authoritative "past", due to some misprediction
    /// error
    RewindEntity(K),
    /// Occurs when a Component should be added to a given Entity
    InsertComponent(K, P),
    /// Occurs when a Component has had a state change on the Server while
    /// the Entity it is attached to has come into scope for the Client
    UpdateComponent(K, P),
    /// Occurs when a Component should be removed from the given Entity
    RemoveComponent(K, P),
    /// An Message emitted to the Client from the Server
    Message(P),
    /// A new Command received immediately to an assigned Entity, used to
    /// extrapolate Entity from the "past" to the "present"
    NewCommand(K, P),
    /// An old Command which has already been received by the assigned Entity,
    /// but which must be replayed after a "ResetEntityPresent" event in order
    /// to extrapolate back to the "present"
    ReplayCommand(K, P),
}
