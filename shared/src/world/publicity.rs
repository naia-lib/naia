/// Publication visibility of an entity — the shared axis used on both sides
/// of the connection.
///
/// On the **server**, `Publicity` is embedded in [`ReplicationConfig`] and
/// controls how an entity replicates to clients.
/// On the **client**, `Publicity` is passed directly to
/// [`EntityMut::configure_replication`].
///
/// The three variants represent the full lifecycle of a client-authoritative
/// entity:
///
/// ```text
/// Server spawns → Delegated
///     ↓  client requests authority
/// Client holds → Private  (not yet published)
///     ↓  client publishes
/// All peers see → Public
///     ↓  client releases or server revokes
/// Server resumes → Delegated / Public
/// ```
///
/// [`ReplicationConfig`]: naia_server::ReplicationConfig
/// [`EntityMut::configure_replication`]: naia_client::EntityMut::configure_replication
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Publicity {
    /// The entity is owned by a client but not yet visible to other peers.
    ///
    /// Used while a client is constructing or preparing an entity before
    /// choosing to publish it. Server-spawned entities are never `Private`.
    Private,
    /// The entity replicates to all peers that share a room and scope with it.
    ///
    /// The default state for server-spawned entities. A client sets an entity
    /// `Public` to make its own client-spawned entity visible to other clients.
    Public,
    /// The server can delegate authority over this entity to a client.
    ///
    /// When a server entity is configured `Delegated`, clients may call
    /// [`entity_request_authority`] to request ownership. The server
    /// grants or denies the request via an [`EntityAuthGrantEvent`] or
    /// [`EntityAuthDeniedEvent`]. While a client holds authority its
    /// mutations replicate back to the server; the server can revoke at
    /// any time.
    ///
    /// [`entity_request_authority`]: naia_client::Client::entity_request_authority
    /// [`EntityAuthGrantEvent`]: naia_client::EntityAuthGrantedEvent
    /// [`EntityAuthDeniedEvent`]: naia_client::EntityAuthDeniedEvent
    Delegated,
}

impl Publicity {
    /// Returns `true` if this is [`Publicity::Delegated`].
    pub fn is_delegated(&self) -> bool {
        matches!(self, Publicity::Delegated)
    }
}
