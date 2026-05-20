//! MISSION_USER_ONLY_SEES_SIM Phase D.2.2 (2026-05-19) — deferred
//! `configure_entity_replication` op model.
//!
//! `WorldServer::configure_entity_replication` (`world_server.rs:1423`)
//! runs a publicity-transition decision tree whose leaves interleave
//! THREE subsystems:
//!
//!   * Coord-side `global_world_manager` (gwm) writes — `entity_publish`,
//!     `entity_unpublish`, `entity_enable_delegation`,
//!     `entity_disable_delegation`, `migrate_entity_to_server`,
//!     `client_request_authority`, `remove_component_diff_handler`,
//!     `entity_set_scope_exit`.
//!   * Send-side per-connection writes — `send_publish` / `send_unpublish`
//!     / `send_enable_delegation` / `send_disable_delegation` command
//!     enqueues, despawn-from-non-owner, the `migrate_entity_remote_to_host`
//!     + `host_local_enable_delegation` + `host_send_migrate_response`
//!     (delegation) sequence, the `host_send_set_auth` fan-out, and the
//!     post-publish scope re-evaluation push.
//!   * World-side hooks — `world.entity_publish` / `world.entity_unpublish`
//!     / `world.entity_enable_delegation` / `world.entity_disable_delegation`
//!     install per-component diff mutators on the (Sim) world.
//!
//! `SimHandle::configure_entity_replication` runs the decision tree
//! ONCE — reading the pre-transition gwm state, performing the gwm
//! writes immediately (so a subsequent same-tick configure on the same
//! entity composes correctly), and recording the Send-side and
//! World-side leaf work as flat ordered op lists captured into the
//! `ScopeChange::ConfigureReplication` payload. The decision tree is NOT
//! re-evaluated in the drain — that would mis-branch off the
//! already-transitioned gwm. Capturing a flat op list (rather than
//! re-running the tree) is the read-before-write fix from B.2 blocker 1:
//! the owner address / user keys / per-conn parameters are snapshotted
//! at push time while gwm still holds the pre-transition state.
//!
//! The Send ops drain in `SendState::apply_pending_configure_replication`
//! (called from `apply_pending_send_preamble`); the World ops drain in
//! `SendHandle::apply_pending_world_hooks<W>` (called from the Sim
//! system, which holds `&mut World`).
//!
//! Wire-byte identity vs the legacy fused-`WorldServer` path is the
//! binding correctness criterion. Each op below is a verbatim relocation
//! of the corresponding statement in the legacy method body — same
//! inputs, same call, same order.

use std::{hash::Hash, net::SocketAddr};

use naia_shared::GlobalEntity;

use crate::UserKey;

/// A Send-side leaf operation deferred out of
/// `configure_entity_replication`'s decision tree. Drained by
/// `SendState::apply_pending_configure_replication`.
///
/// Every variant carries the parameters captured at queue-push time
/// (B.2 blocker 1 read-before-write fix) so the drain never re-reads
/// now-stale gwm state.
pub(crate) enum ConfigureSendOp<E> {
    /// `unpublish_entity` non-owner despawn + owner-targeted
    /// `send_unpublish`. `owner_addr` is captured BEFORE
    /// `gwm.entity_unpublish` transitions ClientPublic → Client, so the
    /// drain can address the formerly-owning connection even though gwm
    /// has already moved on.
    Unpublish {
        global_entity: GlobalEntity,
        /// `Some` when the legacy `server_origin` path would have sent an
        /// `Unpublish` command to the owner. Captured pre-transition.
        owner_addr: Option<SocketAddr>,
    },

    /// `publish_entity`'s owner-targeted `send_publish` (only present
    /// when `server_origin` AND the owner is a `Client`). Captured
    /// owner address resolved from the pre-transition `EntityOwner`.
    Publish {
        global_entity: GlobalEntity,
        owner_addr: SocketAddr,
    },

    /// `publish_entity`'s post-publish scope re-evaluation: re-push
    /// `EntityEnteredRoom` for every room the entity is in. The room
    /// set is read from Send-side `entity_room_map` at drain time
    /// (Send-owned state), so it is resolved in the drain, not captured.
    PublishScopeReeval { global_entity: GlobalEntity },

    /// `entity_enable_delegation`'s per-connection `send_enable_delegation`
    /// fan-out (the loop body before the `client_origin` branch). Skips
    /// the `client_origin` connection (handled by the migration sequence).
    EnableDelegationFanout {
        global_entity: GlobalEntity,
        client_origin: Option<UserKey>,
    },

    /// `enable_delegation_client_owned_entity`'s protocol-ordered Send
    /// sequence: `entity_scope_map` seed, `migrate_entity_remote_to_host`,
    /// `host_local_enable_delegation`, `host_send_migrate_response`
    /// (reserved at subcommand_id=0 via D.2.3's `reserve_first_command`),
    /// then the in-scope `host_send_set_auth` fan-out. Also pushes the
    /// World hooks for this path (publish-race + enable-delegation) in
    /// legacy order, because whether the Client→ClientPublic race-path
    /// fires is only known at drain time. `world_entity` is carried so
    /// those world ops can be enqueued.
    DelegationMigrate {
        global_entity: GlobalEntity,
        world_entity: E,
        client_key: UserKey,
    },

    /// `entity_disable_delegation`'s per-connection `send_disable_delegation`
    /// fan-out.
    DisableDelegationFanout { global_entity: GlobalEntity },
}

/// A World-side hook operation deferred out of
/// `configure_entity_replication`. Drained by
/// `SendHandle::apply_pending_world_hooks<W>` on the Sim system (which
/// holds the `&mut World`). Each maps to a `WorldMutType` hook that
/// installs per-component diff mutators against the post-transition gwm.
pub(crate) enum ConfigureWorldOp<E> {
    Publish { world_entity: E },
    Unpublish { world_entity: E },
    EnableDelegation { world_entity: E },
    DisableDelegation { world_entity: E },
}

/// Payload of `ScopeChange::ConfigureReplication`. Carries the flat
/// ordered Send-op list captured at queue-push time. World ops are held
/// on the separate `pending_world_hooks` queue (different drain site).
pub(crate) struct ConfigureCapture<E> {
    /// Send-side leaf ops, in legacy execution order.
    pub send_ops: Vec<ConfigureSendOp<E>>,
}

impl<E: Copy + Eq + Hash + Send + Sync> std::fmt::Debug for ConfigureWorldOp<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigureWorldOp::Publish { .. } => write!(f, "ConfigureWorldOp::Publish"),
            ConfigureWorldOp::Unpublish { .. } => write!(f, "ConfigureWorldOp::Unpublish"),
            ConfigureWorldOp::EnableDelegation { .. } => {
                write!(f, "ConfigureWorldOp::EnableDelegation")
            }
            ConfigureWorldOp::DisableDelegation { .. } => {
                write!(f, "ConfigureWorldOp::DisableDelegation")
            }
        }
    }
}
