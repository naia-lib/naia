//! `e2e_debug`-only read of the inputs behind one `(user, entity)` scope
//! verdict.
//!
//! [`scope_explain_impl`] reads the same inputs as
//! [`user_scope_has_entity_impl`](super::user_scope_has_entity_impl), and its
//! `has` field is the output of that resolver itself, so it cannot drift
//! from `UserScopeRef::has`. The other fields add the coord-side
//! mirrors and the not-yet-drained staging that the resolver does NOT read,
//! so a diagnostic can tell "coord says the room has it" apart from "the
//! send-side index the resolver reads has it".
//!
//! The fields describe state at read time only. They do not identify which
//! `include()`/`exclude()` call produced an explicit entry.

use std::hash::Hash;

use naia_shared::{
    BigMapKey, EntityAndGlobalEntityConverter, GlobalEntity, Publicity, ResourceRegistry,
};

use crate::{
    server::{
        coord_state::PendingScopeLedgerOp,
        room_store::RoomStore,
        scope_change::{RoomChange, ScopeChange},
        user_store::UserStore,
        ServerShared,
    },
    world::{
        entity_owner::EntityOwner, entity_room_map::EntityRoomMap, entity_scope_map::EntityScopeMap,
    },
    RoomKey, UserKey,
};

/// The inputs behind one `(user, entity)` scope verdict. Room lists are
/// sorted by key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScopeExplain {
    /// The verdict, from the same resolver `UserScopeRef::has` calls.
    pub has: bool,
    /// The user owns this client-owned entity (always in scope).
    pub is_owner: bool,
    /// The entity's publicity is `Private` (never in scope for non-owners).
    pub is_private: bool,
    /// The entity is a replicated resource.
    pub is_resource: bool,
    /// The entity is server-owned.
    pub server_owned: bool,
    /// Send-side explicit entry (`entity_scope_map`), the one the resolver
    /// reads: `Some(true)` include, `Some(false)` exclude, `None` none.
    pub explicit: Option<bool>,
    /// Scope-ledger ops for this pair staged on coord and not yet drained
    /// into `entity_scope_map`, in staging order. Always empty on the
    /// resident engine, which writes the ledger synchronously.
    pub staged: Vec<StagedScopeOp>,
    /// Send-side `entity_room_map` rooms, the ones the resolver reads.
    /// `None` means the map holds no entry (the entity is roomless there).
    pub send_entity_rooms: Option<Vec<RoomKey>>,
    /// Coord-side `RoomStore` rooms holding the entity (what
    /// `room(..).has_entity` reports).
    pub coord_entity_rooms: Vec<RoomKey>,
    /// The user's rooms from the coord `UserStore`, the ones the resolver
    /// reads. `None` means the user is not in the store.
    pub user_rooms: Option<Vec<RoomKey>>,
    /// Room changes for this entity or user still queued on
    /// `scope_change_queue`, i.e. not yet applied to the send-side room
    /// index, in queue order.
    pub queued_room_changes: Vec<QueuedRoomChange>,
}

/// A staged scope-ledger op for the explained pair.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StagedScopeOp {
    /// `include()` (`true`) or `exclude()` (`false`) for this entity.
    Set(bool),
    /// `clear()` for this user: drops every explicit entry of the user.
    RemoveUser,
}

/// An undrained room change touching the explained entity or user.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueuedRoomChange {
    /// The entity was added to this room.
    EntityAdded(RoomKey),
    /// The entity was removed from this room.
    EntityRemoved(RoomKey),
    /// The user was added to this room.
    UserAdded(RoomKey),
    /// The user was removed from this room.
    UserRemoved(RoomKey),
    /// This room, which held the entity, was destroyed.
    RoomDestroyed(RoomKey),
}

impl ScopeExplain {
    /// Recomputes the verdict from the fields alone, following the
    /// resolver's rule order. It must equal `has`; a mismatch means these
    /// fields do not fully explain the verdict.
    pub fn verdict_from_fields(&self) -> bool {
        if self.is_owner {
            return true;
        }
        if self.is_private {
            return false;
        }
        if let Some(included) = self.explicit {
            // [entity-scopes-09]: include() cannot admit a roomless
            // server-owned non-resource.
            if included
                && self.send_entity_rooms.is_none()
                && self.server_owned
                && !self.is_resource
            {
                return false;
            }
            return included;
        }
        match (&self.send_entity_rooms, &self.user_rooms) {
            (Some(entity_rooms), Some(user_rooms)) => {
                entity_rooms.iter().any(|room| user_rooms.contains(room))
            }
            _ => false,
        }
    }
}

fn sorted(rooms: impl Iterator<Item = RoomKey>) -> Vec<RoomKey> {
    let mut rooms: Vec<RoomKey> = rooms.collect();
    rooms.sort_by_key(|room| room.to_u64());
    rooms
}

/// Builds the [`ScopeExplain`] for `(user_key, world_entity)`. The send-side
/// arguments are the ones `user_scope_has_entity_impl` takes; the rest are
/// the coord room store, the coord staging queue and the shared change
/// queue. Mutates nothing. Panics on an unregistered entity, as `has` does.
#[allow(clippy::too_many_arguments)]
pub(crate) fn scope_explain_impl<E: Copy + Eq + Hash + Send + Sync>(
    shared: &ServerShared<E>,
    entity_scope_map: &EntityScopeMap,
    entity_room_map: &EntityRoomMap,
    user_store: &UserStore,
    resource_registry: &ResourceRegistry,
    room_store: &RoomStore,
    pending_scope_ledger_ops: &[PendingScopeLedgerOp<E>],
    user_key: UserKey,
    world_entity: &E,
) -> ScopeExplain {
    let has = super::user_scope_has_entity_impl(
        shared,
        entity_scope_map,
        entity_room_map,
        user_store,
        resource_registry,
        user_key,
        world_entity,
    );

    let global_entity: GlobalEntity = shared
        .global_entity_map
        .read()
        .entity_to_global_entity(world_entity)
        .unwrap();

    let (is_private, owner) = {
        let gwm = shared.global_world_manager.read();
        let is_private = gwm
            .entity_replication_config(global_entity)
            .is_some_and(|config| matches!(config.publicity, Publicity::Private));
        (is_private, gwm.entity_owner(global_entity))
    };
    let is_owner = matches!(
        owner,
        Some(
            EntityOwner::Client(owner_key)
                | EntityOwner::ClientWaiting(owner_key)
                | EntityOwner::ClientPublic(owner_key)
        ) if owner_key == user_key
    );
    let server_owned = owner.is_some_and(|o| o.is_server());

    let staged = pending_scope_ledger_ops
        .iter()
        .filter_map(|op| match op {
            PendingScopeLedgerOp::Set {
                user_key: op_user,
                world_entity: op_entity,
                is_contained,
            } if *op_user == user_key && op_entity == world_entity => {
                Some(StagedScopeOp::Set(*is_contained))
            }
            PendingScopeLedgerOp::RemoveUser { user_key: op_user } if *op_user == user_key => {
                Some(StagedScopeOp::RemoveUser)
            }
            _ => None,
        })
        .collect();

    let coord_entity_rooms = sorted(
        room_store
            .keys()
            .into_iter()
            .filter(|room| room_store.has_entity(*room, global_entity)),
    );

    let queued_room_changes = shared
        .scope_change_queue
        .lock()
        .iter()
        .filter_map(|change| match change {
            ScopeChange::RoomChange(RoomChange::EntityAdded {
                room_key,
                global_entity: g,
                ..
            }) if *g == global_entity => Some(QueuedRoomChange::EntityAdded(*room_key)),
            ScopeChange::RoomChange(RoomChange::EntityRemoved {
                room_key,
                global_entity: g,
                ..
            }) if *g == global_entity => Some(QueuedRoomChange::EntityRemoved(*room_key)),
            ScopeChange::RoomChange(RoomChange::UserAdded {
                room_key,
                user_key: u,
                ..
            }) if *u == user_key => Some(QueuedRoomChange::UserAdded(*room_key)),
            ScopeChange::RoomChange(RoomChange::UserRemoved {
                room_key,
                user_key: u,
            }) if *u == user_key => Some(QueuedRoomChange::UserRemoved(*room_key)),
            ScopeChange::RoomChange(RoomChange::RoomDestroyed {
                room_key,
                removed_entities,
            }) if removed_entities.iter().any(|(_, g)| *g == global_entity) => {
                Some(QueuedRoomChange::RoomDestroyed(*room_key))
            }
            _ => None,
        })
        .collect();

    ScopeExplain {
        has,
        is_owner,
        is_private,
        is_resource: resource_registry.is_resource_entity(global_entity),
        server_owned,
        explicit: entity_scope_map.get(user_key, global_entity).copied(),
        staged,
        send_entity_rooms: entity_room_map
            .entity_get_rooms(global_entity)
            .map(|rooms| sorted(rooms.iter().copied())),
        coord_entity_rooms,
        user_rooms: user_store
            .get(user_key)
            .map(|user| sorted(user.room_keys().iter().copied())),
        queued_room_changes,
    }
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use naia_shared::{BigMapKey, GlobalEntitySpawner, Protocol};

    use super::{QueuedRoomChange, ScopeExplain, StagedScopeOp};
    use crate::{
        server::{world_server::InternalWorldServer, ServerShared},
        world::entity_owner::EntityOwner,
        PipelinedWorldServer, ServerConfig, UserKey, UserScopeMut, UserScopeRef,
    };

    fn protocol() -> Protocol {
        let mut proto = Protocol::builder();
        proto.lock();
        proto.build()
    }

    fn register(shared: &ServerShared<u64>, entity: u64, owner: EntityOwner) {
        let ge = shared.global_entity_map.write().spawn(entity, None);
        let idx = shared
            .global_world_manager
            .write()
            .insert_entity_record(ge, owner);
        if idx.is_valid() {
            shared.idx_to_world.write()[idx.as_usize()] = Some(entity);
        }
    }

    /// The equivalence control: `explain().has` is the verdict `has()`
    /// returns, and the fields alone reproduce it.
    fn explain_pipelined(
        server: &PipelinedWorldServer<u64>,
        user: UserKey,
        e: u64,
    ) -> ScopeExplain {
        let scope = UserScopeRef::with_pipeline(server, user);
        let explain = scope.explain(&e);
        assert_eq!(explain.has, scope.has(&e), "explain().has != has() for {e}");
        assert_eq!(
            explain.verdict_from_fields(),
            explain.has,
            "fields do not explain the verdict for {e}: {explain:?}"
        );
        explain
    }

    fn explain_resident(server: &InternalWorldServer<u64>, user: UserKey, e: u64) -> ScopeExplain {
        let scope = UserScopeRef::new(server, user);
        let explain = scope.explain(&e);
        assert_eq!(explain.has, scope.has(&e), "explain().has != has() for {e}");
        assert_eq!(
            explain.verdict_from_fields(),
            explain.has,
            "fields do not explain the verdict for {e}: {explain:?}"
        );
        explain
    }

    const SHARED: u64 = 1;
    const ELSEWHERE: u64 = 2;
    const ROOMLESS: u64 = 3;
    const OWNED: u64 = 4;

    /// Pipelined engine: every verdict branch, plus the two windows the
    /// resolver cannot see on its own: a room add the send side has not
    /// drained yet, and an exclude staged on coord but not yet applied.
    #[test]
    fn pipelined_explain_matches_has_and_exposes_undrained_state() {
        let mut server = PipelinedWorldServer::<u64>::new(ServerConfig::default(), protocol());
        let user = UserKey::from_u64(5);
        let addr: SocketAddr = "127.0.0.1:20005".parse().unwrap();
        server.receive_user(user, addr);
        for (e, owner) in [
            (SHARED, EntityOwner::Server),
            (ELSEWHERE, EntityOwner::Server),
            (ROOMLESS, EntityOwner::Server),
            (OWNED, EntityOwner::Client(user)),
        ] {
            register(&server.coord().shared, e, owner);
        }
        let room = server.create_room();
        let other_room = server.create_room();
        server.room_add_user(room, user);
        server.room_add_entity(&room, &SHARED);
        server.room_add_entity(&other_room, &ELSEWHERE);

        // Coord holds the membership; the send-side index does not yet.
        let e = explain_pipelined(&server, user, SHARED);
        assert!(!e.has);
        assert_eq!(e.coord_entity_rooms, vec![room]);
        assert_eq!(e.send_entity_rooms, None);
        assert_eq!(e.user_rooms, Some(vec![room]));
        assert_eq!(
            e.queued_room_changes,
            vec![
                QueuedRoomChange::UserAdded(room),
                QueuedRoomChange::EntityAdded(room)
            ]
        );

        let (coord, recv, mut send) = server.take_handles();
        send.state
            .apply_pending_room_changes(&coord.shared.scope_change_queue);
        server.restore_handles(coord, recv, send);

        // Room default, shared room.
        let e = explain_pipelined(&server, user, SHARED);
        assert!(e.has);
        assert_eq!(e.send_entity_rooms, Some(vec![room]));
        assert!(e.queued_room_changes.is_empty());
        assert_eq!(e.explicit, None);

        // Room default, no shared room.
        let e = explain_pipelined(&server, user, ELSEWHERE);
        assert!(!e.has);
        assert_eq!(e.send_entity_rooms, Some(vec![other_room]));

        // Owner wins with no rooms at all.
        let e = explain_pipelined(&server, user, OWNED);
        assert!(e.has && e.is_owner && !e.server_owned);

        // Staged exclude: visible as staged, verdict still the room default.
        UserScopeMut::with_pipeline(&mut server, user).exclude(&SHARED);
        let e = explain_pipelined(&server, user, SHARED);
        assert!(e.has);
        assert_eq!(e.explicit, None);
        assert_eq!(e.staged, vec![StagedScopeOp::Set(false)]);

        // Drained exclude: the explicit entry wins over the shared room.
        server.drain_pending_scope_ledger_ops_for_test();
        let e = explain_pipelined(&server, user, SHARED);
        assert!(!e.has);
        assert_eq!(e.explicit, Some(false));
        assert!(e.staged.is_empty());

        // [entity-scopes-09]: include cannot admit a roomless server entity.
        UserScopeMut::with_pipeline(&mut server, user).include(&ROOMLESS);
        server.drain_pending_scope_ledger_ops_for_test();
        let e = explain_pipelined(&server, user, ROOMLESS);
        assert!(!e.has);
        assert_eq!(e.explicit, Some(true));
        assert_eq!(e.send_entity_rooms, None);
        assert!(e.server_owned && !e.is_resource);

        // clear() is staged as RemoveUser until drained.
        UserScopeMut::with_pipeline(&mut server, user).clear();
        assert_eq!(
            explain_pipelined(&server, user, SHARED).staged,
            vec![StagedScopeOp::RemoveUser]
        );
        server.drain_pending_scope_ledger_ops_for_test();
        let e = explain_pipelined(&server, user, SHARED);
        assert!(e.has);
        assert_eq!(e.explicit, None);
    }

    /// Resident engine: same verdict branches; room changes and scope
    /// writes apply synchronously, so nothing is ever staged or queued.
    #[test]
    fn resident_explain_matches_has() {
        let mut server = InternalWorldServer::<u64>::new(ServerConfig::default(), protocol());
        let user = UserKey::from_u64(6);
        let addr: SocketAddr = "127.0.0.1:20006".parse().unwrap();
        server.receive_user(user, addr);
        for (e, owner) in [
            (SHARED, EntityOwner::Server),
            (ELSEWHERE, EntityOwner::Server),
            (ROOMLESS, EntityOwner::Server),
            (OWNED, EntityOwner::Client(user)),
        ] {
            register(&server.shared, e, owner);
        }
        let room = server.create_room().key();
        let other_room = server.create_room().key();
        server.room_add_user(room, user);
        server.room_add_entity(room, &SHARED);
        server.room_add_entity(other_room, &ELSEWHERE);

        let e = explain_resident(&server, user, SHARED);
        assert!(e.has);
        assert_eq!(e.send_entity_rooms, Some(vec![room]));
        assert_eq!(e.coord_entity_rooms, vec![room]);
        assert!(e.queued_room_changes.is_empty());

        assert!(!explain_resident(&server, user, ELSEWHERE).has);
        assert!(explain_resident(&server, user, OWNED).is_owner);

        server.user_scope_set_entity(user, &SHARED, false);
        let e = explain_resident(&server, user, SHARED);
        assert!(!e.has);
        assert_eq!(e.explicit, Some(false));
        assert!(e.staged.is_empty());

        server.user_scope_set_entity(user, &ROOMLESS, true);
        let e = explain_resident(&server, user, ROOMLESS);
        assert!(!e.has);
        assert_eq!(e.explicit, Some(true));
    }
}
