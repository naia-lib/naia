pub use naia_shared::Publicity;

/// What happens to a user's view of an entity when it leaves their scope.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub enum ScopeExit {
    /// Default: entity is despawned on the client when scope is lost.
    #[default]
    Despawn,
    /// Entity stays in the client's networked entity pool; updates are frozen
    /// until the entity re-enters scope.
    Persist,
}

/// Replication configuration for a server-owned (or migrated) entity.
///
/// Controls two independent axes:
/// - `publicity`: whether/how the entity replicates (Private, Public, Delegated).
/// - `scope_exit`: what happens on the client when the entity leaves a user's scope.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct ReplicationConfig {
    pub publicity: Publicity,
    pub scope_exit: ScopeExit,
}

impl ReplicationConfig {
    /// Public entity with default (Despawn) scope-exit — equivalent to the old `Public` variant.
    pub const fn public() -> Self {
        Self {
            publicity: Publicity::Public,
            scope_exit: ScopeExit::Despawn,
        }
    }

    /// Private entity (unpublished client-owned) — equivalent to the old `Private` variant.
    pub const fn private() -> Self {
        Self {
            publicity: Publicity::Private,
            scope_exit: ScopeExit::Despawn,
        }
    }

    /// Delegated entity — equivalent to the old `Delegated` variant.
    pub const fn delegated() -> Self {
        Self {
            publicity: Publicity::Delegated,
            scope_exit: ScopeExit::Despawn,
        }
    }

    /// Builder: change scope-exit to Persist, keeping publicity unchanged.
    /// When an entity with Persist leaves a user's scope, it stays in the
    /// client's networked entity pool with updates frozen until re-entry.
    pub const fn persist_on_scope_exit(self) -> Self {
        Self {
            scope_exit: ScopeExit::Persist,
            ..self
        }
    }
}
