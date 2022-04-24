use std::net::SocketAddr;

use naia_shared::{ChannelIndex, Protocolize, Tick};

/// An Event that is be emitted by the Client, usually as a result of some
/// communication with the Server
pub enum Event<P: Protocolize, E: Copy, C: ChannelIndex> {
    /// Occurs when the Client has successfully established a connection with
    /// the Server
    Connection(SocketAddr),
    /// Occurs when the Client has lost connection with the Server, usually as a
    /// result of a timeout
    Disconnection(SocketAddr),
    /// A Tick Event, the duration between Tick events is defined in the Config
    /// passed to the Client on initialization
    Tick,
    /// Occurs when an Entity on the Server has come into scope for the Client
    SpawnEntity(E),
    /// Occurs when an Entity on the Server has been destroyed, or left the
    /// Client's scope
    DespawnEntity(E),
    /// Occurs when a Component should be added to a given Entity
    InsertComponent(E, P::Kind),
    /// Occurs when a Component has had a state change on the Server while
    /// the Entity it is attached to has come into scope for the Client
    UpdateComponent(Tick, E, P::Kind),
    /// Occurs when a Component should be removed from the given Entity
    RemoveComponent(E, P),
    /// A Message emitted to the Client from the Server
    Message(C, P),
}
