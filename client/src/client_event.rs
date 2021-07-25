use naia_shared::{EventType, LocalActorKey, LocalEntityKey, LocalComponentKey};

/// An Event that is be emitted by the Client, usually as a result of some
/// communication with the Server
#[derive(Debug)]
pub enum ClientEvent<T: EventType> {
    /// Occurs when the Client has successfully established a connection with
    /// the Server
    Connection,
    /// Occurs when the Client has lost connection with the Server, usually as a
    /// result of a timeout
    Disconnection,
    /// An Event emitted to the Client from the Server
    Event(T),
    /// A Tick Event, the duration between Tick events is defined in the Config
    /// object passed to the Client on initialization
    Tick,
    /// Occurs when an Actor has been assigned to the local host as a Pawn,
    /// meaning it can receive Commands from the Client
    AssignPawn(LocalActorKey),
    /// Occurs when a Pawn Actor has been unassigned from the local host, meaning it
    /// cannot receive Commands from this Client
    UnassignPawn(LocalActorKey),
    /// Occurs when a Pawn Actor needs to be reset to local state
    ResetPawn(LocalActorKey),
    /// A Command received which is to be simulated on the Client as well as on
    /// the Server
    NewCommand(LocalActorKey, T),
    /// A Command which is replayed to extrapolate from recently received
    /// authoritative state
    ReplayCommand(LocalActorKey, T),
    /// Occurs when an Actor on the Server has come into scope for the Client
    CreateActor(LocalActorKey),
    /// Occurs when an Actor has had a state change on the Server while in
    /// scope for the Client
    UpdateActor(LocalActorKey),
    /// Occurs when an Actor on the Server has left the Client's scope
    DeleteActor(LocalActorKey),
    /// Occurs when an Entity on the Server has come into scope for the Client,
    /// and should be added to the local client's ECS "world"
    CreateEntity(LocalEntityKey),
    /// Occurs when an Entity on the Server has left the Client's scope, and should be removed from the local client's ECS "world"
    DeleteEntity(LocalEntityKey),
    /// Occurs when a Component should be added to a given Entity
    AddComponent(LocalEntityKey, LocalComponentKey),
    /// Occurs when a Component has had a state change on the Server while the
    /// Entity it is attached to has come into scope for the Client
    UpdateComponent(LocalEntityKey, LocalComponentKey),
    /// Occurs when a Component should be removed from the given Entity
    RemoveComponent(LocalEntityKey, LocalComponentKey),
    /// Occurs when an Entity has been assigned to the local host as a Pawn,
    /// meaning it can receive Commands from the Client
    AssignPawnEntity(LocalEntityKey),
    /// Occurs when a Pawn Entity has been unassigned from the local host, meaning it
    /// cannot receive Commands from this Client
    UnassignPawnEntity(LocalEntityKey),
    /// Occurs when a Pawn Entity needs to be reset to local state
    ResetPawnEntity(LocalEntityKey),
    /// An Pawn Entity Command received which is to be simulated on the Client as well as on
    /// the Server
    NewCommandEntity(LocalEntityKey, T),
    /// An Pawn Entity Command which is replayed to extrapolate from recently received
    /// authoritative state
    ReplayCommandEntity(LocalEntityKey, T),
}
