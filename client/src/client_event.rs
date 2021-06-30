use naia_shared::{EventType, LocalActorKey};

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
    /// Occurs when an Actor on the Server has come into scope for the Client
    CreateActor(LocalActorKey),
    /// Occurs when an Actor has had a state change on the Server while in
    /// scope for the Client
    UpdateActor(LocalActorKey),
    /// Occurs when an Actor on the Server has left the Client's scope
    DeleteActor(LocalActorKey),
    /// A Tick Event, the duration between Tick events is defined in the Config
    /// object passed to the Client on initialization
    Tick,
    /// Occurs when an Actor has been assigned to the local host as a Pawn,
    /// meaning it can receive Commands from the Client
    AssignPawn(LocalActorKey),
    /// Occurs when a Pawn has been unassigned from the local host, meaning it
    /// cannot receive Commands from this Client
    UnassignPawn(LocalActorKey),
    /// Occurs when a Pawn needs to be reset to local
    ResetPawn(LocalActorKey),
    /// A Command received which is to be simulated on the Client as well as on
    /// the Server
    Command(LocalActorKey, T),
}
