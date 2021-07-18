use naia_shared::EntityKey;

use super::{
    actors::actor_key::actor_key::ActorKey,
    user::{user_key::UserKey, User},
};

/// An Event that is emitted as a result of some communication with a Client, or
/// a Tick event
pub enum ServerEvent<T> {
    /// Occurs when a new Client has successfully established a connection with
    /// the Server
    Connection(UserKey),
    /// Occurs when the Server has lost connection to a Client, usually as the
    /// result of a timeout
    Disconnection(UserKey, User),
    /// An Event emitted to the Server from a Client
    Event(UserKey, T),
    /// A Tick Event, the duration between Tick events is defined in the Config
    /// object passed to the Server on initialization
    Tick,
    /// An Command emitted to the Server from a Client
    Command(UserKey, ActorKey, T),
    /// Event which is fired when an Actor comes into scope for a given User
    IntoScope(UserKey, ActorKey),
    /// Event which is fired when an Actor goes out of scope for a given User
    OutOfScope(UserKey, ActorKey),
    /// An Command emitted to the Server from a Client, related to an Entity
    CommandEntity(UserKey, EntityKey, T),
    /// Event which is fired when an Entity comes into scope for a given User
    IntoScopeEntity(UserKey, EntityKey),
    /// Event which is fired when an Entity goes out of scope for a given User
    OutOfScopeEntity(UserKey, EntityKey),
}
