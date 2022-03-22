use naia_shared::{ChannelIndex, Protocolize};

use super::user::{User, UserKey};

/// An Event that is emitted as a result of some communication with a Client, or
/// a Tick event
pub enum Event<P: Protocolize, C: ChannelIndex> {
    /// Occurs when a Client attempts to establish a connection with the Server.
    /// Used accept or reject incoming Clients
    Authorization(UserKey, P),
    /// Occurs when a new Client has successfully established a connection with
    /// the Server
    Connection(UserKey),
    /// Occurs when the Server has lost connection to a Client, usually as the
    /// result of a timeout
    Disconnection(UserKey, User),
    /// A Tick Event.
    /// The duration between Tick events is defined in the Config passed to the
    /// Server on initialization
    Tick,
    /// A Message emitted to the Server from a Client
    Message(UserKey, C, P),
}
