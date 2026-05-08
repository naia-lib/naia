pub use naia_shared::Publicity;

/// What happens to a client's view of an entity when it leaves their scope.
///
/// Scope is lost when either the entity or the user is removed from a shared
/// room, or when a [`UserScopeMut`] explicitly excludes the entity.
///
/// [`UserScopeMut`]: crate::UserScopeMut
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub enum ScopeExit {
    /// The entity is despawned on the client when scope is lost.
    ///
    /// This is the default. When the entity re-enters scope it is spawned
    /// again from scratch with a full component snapshot.
    #[default]
    Despawn,
    /// The entity remains in the client's networked entity pool when scope is
    /// lost, but component updates are frozen until the entity re-enters scope.
    ///
    /// Use `Persist` for entities the client already knows about that will
    /// temporarily leave and re-enter scope — this avoids the cost of a full
    /// respawn and prevents the client from observing a "blip" in entity
    /// existence.
    Persist,
}

/// Replication configuration for a server entity.
///
/// Two orthogonal axes govern how the entity behaves on connected clients:
///
/// - [`publicity`](ReplicationConfig::publicity) — controls *who* can see or
///   mutate the entity. See [`Publicity`] for the full state machine.
/// - [`scope_exit`](ReplicationConfig::scope_exit) — controls what happens on
///   a client when the entity leaves that user's scope. See [`ScopeExit`].
///
/// Use the const constructors [`public`](ReplicationConfig::public),
/// [`private`](ReplicationConfig::private), and
/// [`delegated`](ReplicationConfig::delegated) as starting points, then chain
/// [`persist_on_scope_exit`](ReplicationConfig::persist_on_scope_exit) to
/// override the scope-exit behaviour.
///
/// # Examples
///
/// ```rust
/// # use naia_server::ReplicationConfig;
/// // Public entity that persists in scope when temporarily out of view:
/// let cfg = ReplicationConfig::public().persist_on_scope_exit();
///
/// // Delegated entity with default (despawn) scope-exit:
/// let cfg = ReplicationConfig::delegated();
/// ```
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct ReplicationConfig {
    /// Visibility and authority mode for this entity.
    pub publicity: Publicity,
    /// Behaviour when the entity leaves a user's scope.
    pub scope_exit: ScopeExit,
}

impl ReplicationConfig {
    /// Creates a [`Public`](Publicity::Public) config with
    /// [`Despawn`](ScopeExit::Despawn) scope-exit.
    ///
    /// This is the default for server-spawned entities.
    pub const fn public() -> Self {
        Self {
            publicity: Publicity::Public,
            scope_exit: ScopeExit::Despawn,
        }
    }

    /// Creates a [`Private`](Publicity::Private) config with
    /// [`Despawn`](ScopeExit::Despawn) scope-exit.
    ///
    /// Used for client-spawned entities that have not yet been published to
    /// other peers.
    pub const fn private() -> Self {
        Self {
            publicity: Publicity::Private,
            scope_exit: ScopeExit::Despawn,
        }
    }

    /// Creates a [`Delegated`](Publicity::Delegated) config with
    /// [`Despawn`](ScopeExit::Despawn) scope-exit.
    ///
    /// Marks the entity as open for client authority requests. Clients may
    /// call [`entity_request_authority`] to request ownership; the server
    /// grants or denies via an event.
    ///
    /// [`entity_request_authority`]: naia_client::Client::entity_request_authority
    pub const fn delegated() -> Self {
        Self {
            publicity: Publicity::Delegated,
            scope_exit: ScopeExit::Despawn,
        }
    }

    /// Returns a copy of this config with [`scope_exit`](ScopeExit::Persist)
    /// set to [`Persist`](ScopeExit::Persist), leaving `publicity` unchanged.
    ///
    /// When this entity leaves a user's scope it stays in their networked
    /// entity pool with updates frozen, rather than being despawned and
    /// respawned on re-entry.
    pub const fn persist_on_scope_exit(self) -> Self {
        Self {
            scope_exit: ScopeExit::Persist,
            ..self
        }
    }
}
