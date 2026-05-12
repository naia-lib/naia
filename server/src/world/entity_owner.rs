use crate::UserKey;

/// The authority origin of a server-tracked entity.
///
/// Every entity the server knows about has an owner, which determines whose
/// mutations are authoritative and how the entity is replicated to other peers.
///
/// Client-spawned entities move through the variants as they progress from
/// private construction to public visibility:
///
/// ```text
/// client spawns → Client (Private)
///     ↓  client publishes
/// ClientWaiting  (publish in-flight)
///     ↓  server confirms
/// ClientPublic   (visible to peers in scope)
///     ↓  client despawns or disconnects
/// (removed)
/// ```
///
/// Server-spawned entities stay in [`Server`](EntityOwner::Server) for their
/// entire lifetime unless authority is delegated — in which case a separate
/// [`EntityAuthStatus`] tracks the delegation state rather than the owner
/// variant.
///
/// [`EntityAuthStatus`]: naia_shared::EntityAuthStatus
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum EntityOwner {
    /// Spawned and owned by the server.
    ///
    /// The server is the authoritative source of all component state. The
    /// entity may optionally be marked [`Delegated`] in its
    /// [`ReplicationConfig`], allowing a client to request temporary
    /// write authority.
    ///
    /// [`Delegated`]: naia_shared::Publicity::Delegated
    /// [`ReplicationConfig`]: crate::ReplicationConfig
    Server,
    /// Spawned by the given client and currently [`Private`].
    ///
    /// The entity exists on the server but has not yet been published; other
    /// clients cannot see it. The owning client holds write authority.
    ///
    /// [`Private`]: naia_shared::Publicity::Private
    Client(UserKey),
    /// Spawned by the given client; publication is in-flight.
    ///
    /// The client has requested to publish the entity but the server has not
    /// yet confirmed the transition to [`ClientPublic`](EntityOwner::ClientPublic).
    /// Component mutations from the owning client are still authoritative during
    /// this window.
    ClientWaiting(UserKey),
    /// Spawned by the given client and currently [`Public`].
    ///
    /// The entity replicates to all peers that share a room and scope with it.
    /// The owning client retains write authority.
    ///
    /// [`Public`]: naia_shared::Publicity::Public
    ClientPublic(UserKey),
    /// A local-only entity that is never replicated to any client.
    ///
    /// Useful for server-side bookkeeping objects that should participate in
    /// the same entity infrastructure (component storage, etc.) without being
    /// transmitted over the network.
    Local,
}

impl EntityOwner {
    /// Returns `true` if this entity is owned by the server.
    pub fn is_server(&self) -> bool {
        matches!(self, EntityOwner::Server)
    }

    /// Returns `true` if this entity was spawned by a client (regardless of
    /// its current publication state).
    pub fn is_client(&self) -> bool {
        matches!(
            self,
            EntityOwner::Client(_) | EntityOwner::ClientPublic(_) | EntityOwner::ClientWaiting(_)
        )
    }

    /// Returns `true` if this entity is currently visible to other peers.
    ///
    /// Server-owned entities and [`ClientPublic`](EntityOwner::ClientPublic)
    /// entities are public. [`Client`](EntityOwner::Client) (private),
    /// [`ClientWaiting`](EntityOwner::ClientWaiting), and
    /// [`Local`](EntityOwner::Local) are not.
    pub fn is_public(&self) -> bool {
        matches!(self, EntityOwner::ClientPublic(_) | EntityOwner::Server)
    }
}
