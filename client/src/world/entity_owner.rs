/// The authority origin of a client-tracked entity.
///
/// The client assigns an owner to every entity it knows about. This mirrors
/// the server-side [`EntityOwner`] but uses only the variants observable from
/// the client's perspective.
///
/// [`EntityOwner`]: naia_server::EntityOwner
#[derive(Copy, Clone, PartialEq, Eq)]
pub enum EntityOwner {
    /// Originated on the server and replicated to this client.
    ///
    /// The server is the authoritative source of all component state. The
    /// entity may be [`Delegated`], in which case this client (or another)
    /// may hold temporary write authority — see [`EntityRef::authority`].
    ///
    /// [`Delegated`]: naia_shared::Publicity::Delegated
    /// [`EntityRef::authority`]: crate::EntityRef::authority
    Server,
    /// Spawned by this client.
    ///
    /// While [`Private`](naia_shared::Publicity::Private) the entity is
    /// only visible to the owning client. After the client publishes it
    /// ([`Public`](naia_shared::Publicity::Public)) it replicates to peers
    /// in the same room and scope.
    Client,
    /// A local-only entity that is never replicated to the server.
    ///
    /// Exists solely in the client's local world; the server has no knowledge
    /// of it.
    Local,
}

impl EntityOwner {
    /// Returns `true` if this entity originated on the server.
    pub fn is_server(&self) -> bool {
        matches!(self, EntityOwner::Server)
    }

    /// Returns `true` if this entity was spawned by this client.
    pub fn is_client(&self) -> bool {
        matches!(self, EntityOwner::Client)
    }
}
