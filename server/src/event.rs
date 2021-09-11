use naia_shared::EntityKey;

use super::{
    user::{user_key::UserKey, User},
};

/// An Event that is emitted as a result of some communication with a Client, or
/// a Tick event
pub enum Event<T> {
    /// Occurs when a Client attempts to establish a connection with the Server.
    /// Used accept or reject incoming Clients
    Authorization(UserKey, T),
    /// Occurs when a new Client has successfully established a connection with
    /// the Server
    Connection(UserKey),
    /// Occurs when the Server has lost connection to a Client, usually as the
    /// result of a timeout
    Disconnection(UserKey, User),
    /// A Message emitted to the Server from a Client
    Message(UserKey, T),
    /// A Tick Event, the duration between Tick events is defined in the Config
    /// passed to the Server on initialization
    Tick,
    /// A Command emitted to the Server from a Client
    //Command(UserKey, ObjectKey, T),
    /// A Command emitted to the Server from a Client, related to an Entity
    CommandEntity(UserKey, EntityKey, T),
}
