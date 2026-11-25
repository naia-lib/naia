use super::*;

impl<E: Copy + Eq + Hash + Send + Sync> InternalWorldServer<E> {
    // Crate-Public methods

    //// Entities

    /// Despawns the Entity, if it exists.
    /// This will also remove all of the Entity’s Components.
    /// Panics if the Entity does not exist.
    pub(crate) fn despawn_entity<W: WorldMutType<E>>(&mut self, world: &mut W, world_entity: &E) {
        assert!(
            world.has_entity(world_entity),
            "attempted to de-spawn nonexistent entity"
        );

        // Delete from world
        world.despawn_entity(world_entity);

        self.despawn_entity_worldless(world_entity);
    }

    /// Removes an entity from all replication state without touching the world (adapter use only).
    pub fn despawn_entity_worldless(&mut self, world_entity: &E) {
        let Ok(global_entity) = self
            .shared
            .global_entity_map
            .read()
            .entity_to_global_entity(world_entity)
        else {
            return;
        };
        // Priority layer eviction: drop global entry + every user's per-user
        // entry for this entity. Prevents leaks across entity lifetime.
        // 4-E.2e: evict from BOTH mirror (borrow-API source) and the
        // send-side authoritative copy — publish-on-read at next
        // `send_all_packets` would also clear it, but doing it eagerly
        // here keeps the two copies bit-identical between ticks.
        self.sim_handle
            .state
            .global_priority_mirror
            .on_despawn(world_entity);
        self.send.state.global_priority.on_despawn(world_entity);
        for layer in self.send.state.user_priorities.values_mut() {
            layer.on_scope_exit(world_entity);
        }
        // Drop every (*, *, world_entity) tuple from the scope-checks cache.
        // Single linear retain — covers all rooms that previously contained
        // the entity, replacing what would otherwise be one retain per
        // affected room.
        self.send
            .state
            .scope_checks_cache
            .on_entity_despawned(*world_entity);
        self.cleanup_entity_replication(global_entity);
        self.shared
            .global_world_manager
            .write()
            .remove_entity_record(global_entity);
        self.shared
            .global_entity_map
            .write()
            .despawn_by_global(global_entity);
    }

    fn cleanup_entity_replication(&mut self, global_entity: GlobalEntity) {
        self.despawn_entity_from_all_connections(global_entity);

        // Delete scope
        self.send
            .state
            .entity_scope_map
            .remove_entity(global_entity);

        // Delete room cache entry
        if let Some(room_keys) = self
            .send
            .state
            .entity_room_map
            .remove_from_all_rooms(global_entity)
        {
            for room_key in room_keys {
                if let Some(room) = self.sim_handle.state.room_store.get_mut(room_key) {
                    room.remove_entity(global_entity, true);
                }
            }
        }

        // Remove from ECS Record
        self.shared
            .global_world_manager
            .write()
            .remove_entity_diff_handlers(global_entity);
    }

    fn despawn_entity_from_all_connections(&mut self, global_entity: GlobalEntity) {
        // TODO: we can make this more efficient in the future by caching which Entities
        // are in each User's scope
        let entity_idx = self.entity_global_idx(global_entity);
        if entity_idx.is_valid() {
            self.shared.set_idx_to_world(entity_idx, None);
        }
        for send_conn in self.send.state.send_user_connections.values_mut() {
            if !send_conn
                .base
                .world_manager
                .has_global_entity(global_entity)
            {
                continue;
            }
            // remove entity from user connection
            send_conn.base.world_manager.despawn_entity(global_entity);
            send_conn.clear_entity_visible(entity_idx);
        }
    }

    //// Entity Scopes

    /// Remove all entities from a User's scope
    pub(crate) fn user_scope_remove_user(&mut self, user_key: UserKey) {
        self.send.state.entity_scope_map.remove_user(user_key);
    }

    /// Arm the one-shot scope-exit override for `(user_key, world_entity)`.
    ///
    /// No-op if the entity is not registered with this server's
    /// `global_entity_map`. See [`UserScopeMut::despawn_on_next_exit`].
    pub(crate) fn user_scope_despawn_on_next_exit(&mut self, user_key: UserKey, world_entity: &E) {
        let Ok(global_entity) = self
            .shared
            .global_entity_map
            .read()
            .entity_to_global_entity(world_entity)
        else {
            return;
        };
        self.send
            .state
            .entity_scope_map
            .set_despawn_on_next_exit(user_key, global_entity);
    }

    #[expect(clippy::unwrap_used, reason = "caller holds a registered entity")]
    pub(crate) fn user_scope_set_entity(
        &mut self,
        user_key: UserKey,
        world_entity: &E,
        is_contained: bool,
    ) {
        let global_entity = self
            .shared
            .global_entity_map
            .read()
            .entity_to_global_entity(world_entity)
            .unwrap();

        // Per [entity-authority-12]: If the authority-holding client loses scope for E,
        // the server MUST release/reset authority for E.
        // Check if user is being removed from scope and is the authority holder
        // 4-F.naia.a: drop the read guard explicitly via a let binding so it
        // cannot remain alive into the if body's write-guard acquisition
        // (parking_lot RwLock would deadlock on the upgrade).
        let is_authority_holder = self
            .shared
            .global_world_manager
            .read()
            .user_is_authority_holder(user_key, global_entity);
        if !is_contained && is_authority_holder {
            // Release authority - the user is losing scope while holding authority
            let releaser = AuthOwner::Client(user_key);
            if self
                .shared
                .global_world_manager
                .write()
                .client_release_authority(global_entity, &releaser)
                .is_ok()
            {
                // Notify other clients that authority is now Available
                self.send_reset_authority_messages(global_entity);
            }
        }

        // Per [entity-publication]: silently ignore explicit include() for Private entities
        // when the user is not the owner — mirrors the guard in user_scope_has_entity().
        if is_contained {
            let is_private = self
                .shared
                .global_world_manager
                .read()
                .entity_replication_config(global_entity)
                .is_some_and(|c| matches!(c.publicity, Publicity::Private));
            if is_private {
                let is_owner = match self
                    .shared
                    .global_world_manager
                    .read()
                    .entity_owner(global_entity)
                {
                    Some(
                        EntityOwner::Client(owner_key)
                        | EntityOwner::ClientWaiting(owner_key)
                        | EntityOwner::ClientPublic(owner_key),
                    ) => owner_key == user_key,
                    _ => false,
                };
                if !is_owner {
                    return;
                }
            }
        }

        self.send
            .state
            .entity_scope_map
            .insert(user_key, global_entity, is_contained);
        self.shared
            .scope_change_queue
            .lock()
            .push_back(ScopeChange::ScopeToggled(
                user_key,
                global_entity,
                is_contained,
            ));
    }

    pub(crate) fn user_scope_has_entity(&self, user_key: UserKey, world_entity: &E) -> bool {
        // task #9: the canonical body is factored into a free function so the
        // pipelined `&self` slot-lock read path
        // (`PipelinedWorldServer::user_scope_has_entity_ref`) shares it verbatim
        // — zero semantic drift between the fused and split engines.
        crate::server::user_scope_has_entity_impl(
            &self.shared,
            &self.send.state.entity_scope_map,
            &self.send.state.entity_room_map,
            &self.sim_handle.state.user_store,
            &self.sim_handle.state.resource_registry,
            user_key,
            world_entity,
        )
    }

    /// `e2e_debug` read of the inputs behind [`Self::user_scope_has_entity`].
    #[cfg(feature = "e2e_debug")]
    pub(crate) fn user_scope_explain(
        &self,
        user_key: UserKey,
        world_entity: &E,
    ) -> crate::server::ScopeExplain {
        crate::server::scope_explain::scope_explain_impl(
            &self.shared,
            &self.send.state.entity_scope_map,
            &self.send.state.entity_room_map,
            &self.sim_handle.state.user_store,
            &self.sim_handle.state.resource_registry,
            &self.sim_handle.state.room_store,
            &self.sim_handle.state.pending_scope_ledger_ops,
            user_key,
            world_entity,
        )
    }

    //// Components

    /// Adds a Component to an Entity
    pub(crate) fn insert_component<R: ReplicatedComponent, W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        world_entity: &E,
        mut component: R,
    ) {
        assert!(
            world.has_entity(world_entity),
            "attempted to add component to non-existent entity"
        );

        let component_kind = component.kind();

        if world.has_component_of_kind(world_entity, component_kind) {
            // Entity already has this Component type yet, update Component

            let Some(mut component_mut) = world.component_mut::<R>(world_entity) else {
                panic!("Should never happen because we checked for this above");
            };
            component_mut.mirror(&component);
        } else {
            // Entity does not have this Component type yet, initialize Component
            self.insert_component_worldless(world_entity, &mut component);

            // actually insert component into world
            world.insert_component(world_entity, component);
        }
    }

    /// Registers a component insertion in the replication layer (adapter use only); panics if `world_entity` is not registered.
    #[expect(clippy::unwrap_used, reason = "caller holds a registered entity")]
    pub fn insert_component_worldless(&mut self, world_entity: &E, component: &mut dyn Replicate) {
        let component_kind = component.kind();

        let global_entity = self
            .shared
            .global_entity_map
            .read()
            .entity_to_global_entity(world_entity)
            .unwrap();

        if self
            .shared
            .global_world_manager
            .read()
            .has_component_record(global_entity, &component_kind)
        {
            warn!(
                "Attempted to add component `{:?}` to entity `{:?}` that already has it, \
                 this can happen if a delegated entity's auth is transferred to the Server \
                 before the Server Adapter has been able to process the newly inserted \
                 Component. Skipping this action.",
                component.name(),
                global_entity,
            );
            return;
        }

        // Order matters: register the component GLOBALLY before propagating to
        // per-user scopes. `insert_new_component_into_entity_scopes` triggers
        // per-user `UserDiffHandler::register_component` calls that depend on
        // the global MutChannel existing — without it, `global_handler.receiver`
        // returns None and the per-user notifier is never attached, so mutations
        // on this component for already-in-scope users never reach the per-user
        // diff mask. Symptom: late-inserted components (e.g. NetworkedLastCommand
        // added to an avatar after the entity was already in scope for the
        // observer) silently never replicate their value changes.
        self.shared
            .global_world_manager
            .write()
            .insert_component_record(global_entity, &component_kind);
        self.shared
            .global_world_manager
            .write()
            .insert_component_diff_handler(&self.shared.component_kinds, global_entity, component);
        self.insert_new_component_into_entity_scopes(global_entity, &component_kind, None);

        // if entity is delegated, convert over
        if self
            .shared
            .global_world_manager
            .read()
            .entity_is_delegated(global_entity)
        {
            let accessor = self
                .shared
                .global_world_manager
                .read()
                .get_entity_auth_accessor(global_entity);
            component.enable_delegation(&accessor, None);
        }
    }

    pub(super) fn insert_new_component_into_entity_scopes(
        &mut self,
        global_entity: GlobalEntity,
        component_kind: &ComponentKind,
        excluding_user_opt: Option<UserKey>,
    ) {
        let excluding_addr_opt: Option<SocketAddr> = {
            if let Some(user_key) = excluding_user_opt {
                self.sim_handle
                    .state
                    .user_store
                    .get(user_key)
                    .map(crate::user::world_user::WorldUser::address)
            } else {
                None
            }
        };
        // add component to connections already tracking entity
        for (addr, send_conn) in &mut self.send.state.send_user_connections {
            if let Some(exclude_addr) = excluding_addr_opt {
                if addr == &exclude_addr {
                    continue;
                }
            }

            // insert component into user's connection
            let has_entity = send_conn
                .base
                .world_manager
                .has_global_entity(global_entity);

            if !has_entity {
                // entity is not in scope for this connection
                continue;
            }
            send_conn
                .base
                .world_manager
                .insert_component(global_entity, *component_kind);
        }
    }

    /// Removes a Component from an Entity
    pub(crate) fn remove_component<R: ReplicatedComponent, W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        world_entity: &E,
    ) -> Option<R> {
        self.remove_component_worldless(world_entity, &ComponentKind::of::<R>());

        // remove from world
        world.remove_component::<R>(world_entity)
    }

    /// Removes a component from the replication layer (adapter use only); panics if `world_entity` is not registered.
    #[expect(clippy::unwrap_used, reason = "caller holds a registered entity")]
    pub fn remove_component_worldless(&mut self, world_entity: &E, component_kind: &ComponentKind) {
        let global_entity = self
            .shared
            .global_entity_map
            .read()
            .entity_to_global_entity(world_entity)
            .unwrap();
        self.remove_component_from_all_connections(global_entity, component_kind);

        // cleanup all other loose ends
        self.shared
            .global_world_manager
            .write()
            .remove_component_record(global_entity, component_kind);
        self.shared
            .global_world_manager
            .write()
            .remove_component_diff_handler(global_entity, component_kind);
    }

    fn remove_component_from_all_connections(
        &mut self,
        global_entity: GlobalEntity,
        component_kind: &ComponentKind,
    ) {
        // TODO: should be able to make this more efficient by caching for every Entity
        // which scopes they are part of
        for send_conn in self.send.state.send_user_connections.values_mut() {
            if !send_conn
                .base
                .world_manager
                .has_global_entity(global_entity)
            {
                // entity is not in scope for this connection
                continue;
            }
            // remove component from user connection
            send_conn
                .base
                .world_manager
                .remove_component(global_entity, *component_kind);
        }
    }

    //// Authority

    pub(crate) fn publish_entity<W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        global_entity: GlobalEntity,
        world_entity: &E,
        server_origin: bool,
    ) -> bool {
        if server_origin {
            // send publish message to entity owner
            let entity_owner = self
                .shared
                .global_world_manager
                .read()
                .entity_owner(global_entity);
            let Some(EntityOwner::Client(user_key)) = entity_owner else {
                panic!(
                    "Entity is not owned by a Client. Cannot publish entity. Owner is: {entity_owner:?}"
                );
            };
            // Send PublishEntity action through EntityActionEvent system
            if let Some(user) = self.sim_handle.state.user_store.get(user_key) {
                if let Some(send_conn) = self
                    .send
                    .state
                    .send_user_connections
                    .get_mut(&user.address())
                {
                    send_conn
                        .base
                        .world_manager
                        .send_publish(HostType::Server, global_entity);
                }
            }
        }

        let result = self
            .shared
            .global_world_manager
            .write()
            .entity_publish(global_entity);
        if result {
            let entity_map = self.shared.global_entity_map.read();
            world.entity_publish(
                &self.shared.component_kinds,
                &*entity_map,
                &*self.shared.global_world_manager.read(),
                world_entity,
            );
            // Re-evaluate scope for every user who shares a room with this entity.
            // The EntityEnteredRoom change was already processed when Private (and
            // returned early); now that the entity is Public we must trigger it again.
            let entity_rooms: Vec<RoomKey> = self
                .send
                .state
                .entity_room_map
                .entity_get_rooms(global_entity)
                .map(|rooms| rooms.iter().copied().collect())
                .unwrap_or_default();
            if !entity_rooms.is_empty() {
                let mut q = self.shared.scope_change_queue.lock();
                for room_key in entity_rooms {
                    q.push_back(ScopeChange::EntityEnteredRoom(global_entity, room_key));
                }
            }
        }
        result
    }

    pub(crate) fn unpublish_entity<W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        global_entity: GlobalEntity,
        world_entity: &E,
        server_origin: bool,
    ) {
        // Capture the owner's connection address before state change.
        // entity_unpublish() transitions the owner from ClientPublic → Client,
        // so we read it here while it is still ClientPublic.
        let owner_addr: Option<SocketAddr> = self
            .shared
            .global_world_manager
            .read()
            .entity_owner(global_entity)
            .and_then(|o| {
                if let EntityOwner::ClientPublic(k) = o {
                    Some(k)
                } else {
                    None
                }
            })
            .and_then(|k| {
                self.sim_handle
                    .state
                    .user_store
                    .get(k)
                    .map(crate::user::world_user::WorldUser::address)
            });

        if server_origin {
            // Send UnpublishEntity action through EntityActionEvent system
            if let Some(addr) = owner_addr {
                if let Some(send_conn) = self.send.state.send_user_connections.get_mut(&addr) {
                    send_conn
                        .base
                        .world_manager
                        .send_unpublish(HostType::Server, global_entity);
                }
            }
        }

        self.shared
            .global_world_manager
            .write()
            .entity_unpublish(global_entity);
        world.entity_unpublish(world_entity);

        // Deregister each component from the diff handler so re-publishing
        // can register them again without the "cannot Register more than once" panic.
        // 4-F.naia.a: clone the component kinds out so the read guard
        // drops before the .write() acquisition in the loop body
        // (parking_lot RwLock would deadlock on read-held-during-write).
        let kinds_opt: Option<Vec<naia_shared::ComponentKind>> = self
            .shared
            .global_world_manager
            .read()
            .component_kinds(global_entity)
            .map(|k| k.into_iter().collect());
        if let Some(kinds) = kinds_opt {
            for component_kind in kinds {
                self.shared
                    .global_world_manager
                    .write()
                    .remove_component_diff_handler(global_entity, &component_kind);
            }
        }

        // Despawn from non-owner connections only.  Scope map entries and room
        // membership are preserved so a subsequent publish_entity call restores
        // non-owner visibility via room-based scope (entity-publication-11).
        let entity_idx = self.entity_global_idx(global_entity);
        for (addr, send_conn) in &mut self.send.state.send_user_connections {
            if owner_addr == Some(*addr) {
                continue;
            }
            if send_conn
                .base
                .world_manager
                .has_global_entity(global_entity)
            {
                send_conn.base.world_manager.despawn_entity(global_entity);
                send_conn.clear_entity_visible(entity_idx);
            }
        }
    }
    pub(crate) fn entity_enable_delegation<W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        global_entity: GlobalEntity,
        world_entity: &E,
        client_origin: Option<UserKey>,
    ) {
        // TODO: check that entity is eligible for delegation?

        {
            // For any users that have this entity in scope,
            // Send an `enable_delegation` message

            // TODO: we can make this more efficient in the future by caching which Entities
            // are in each User's scope
            for (user_key, user) in self.sim_handle.state.user_store.iter() {
                if let Some(client_key) = &client_origin {
                    if user_key == client_key {
                        // skip sending to origin client, will be handled below
                        continue;
                    }
                }

                let Some(send_conn) = self
                    .send
                    .state
                    .send_user_connections
                    .get_mut(&user.address())
                else {
                    continue;
                };

                if !send_conn
                    .base
                    .world_manager
                    .has_global_entity(global_entity)
                {
                    // entity is not in scope for this connection
                    continue;
                }

                // Send EnableDelegationEntity action through EntityActionEvent system
                info!(
                    "Sending EnableDelegation command for entity: {:?} for user: {:?}",
                    global_entity,
                    user.address()
                );
                send_conn.base.world_manager.send_enable_delegation(
                    HostType::Server,
                    client_origin.is_some(),
                    global_entity,
                );
            }
        }

        if let Some(client_key) = client_origin {
            self.enable_delegation_client_owned_entity(
                world,
                global_entity,
                world_entity,
                client_key,
            );
        } else {
            self.shared
                .global_world_manager
                .write()
                .entity_enable_delegation(global_entity);
            let entity_map = self.shared.global_entity_map.read();
            world.entity_enable_delegation(
                &self.shared.component_kinds,
                &*entity_map,
                &*self.shared.global_world_manager.read(),
                world_entity,
            );
        }
    }

    pub(crate) fn enable_delegation_client_owned_entity<W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        global_entity: GlobalEntity,
        world_entity: &E,
        client_key: UserKey,
    ) {
        let Some(entity_owner) = self
            .shared
            .global_world_manager
            .read()
            .entity_owner(global_entity)
        else {
            panic!("entity should have an owner at this point");
        };
        let owner_user_key;
        match entity_owner {
            EntityOwner::Client(user_key) => {
                // The entity was spawned by the client but the Publish packet
                // has not yet arrived (enable-delegation arrived first due to
                // packet reordering). Promote the entity to ClientPublic now so
                // delegation setup can proceed — the Publish packet, when it
                // arrives, will be a no-op since the entity is already public.
                // This is the correct handling for the publish-after-delegation
                // packet-ordering race; it is NOT a shortcut around the protocol.
                owner_user_key = user_key;
                let result = self
                    .shared
                    .global_world_manager
                    .write()
                    .entity_publish(global_entity);
                if !result {
                    warn!(
                        "enable_delegation_client_owned_entity: entity_publish failed for {global_entity:?}; \
                         aborting delegation enable (entity may already be public or in an \
                         inconsistent state)"
                    );
                    return;
                }
                let entity_map = self.shared.global_entity_map.read();
                world.entity_publish(
                    &self.shared.component_kinds,
                    &*entity_map,
                    &*self.shared.global_world_manager.read(),
                    world_entity,
                );
            }
            EntityOwner::ClientPublic(user_key) => {
                owner_user_key = user_key;
            }
            _owner => {
                panic!(
                    "entity should be owned by a public client at this point. Owner is: {entity_owner:?}"
                );
            }
        }
        let user_key = owner_user_key;
        self.shared
            .global_world_manager
            .write()
            .migrate_entity_to_server(global_entity);

        // Initialize the former-owner's scope entry to "in scope" only if it
        // wasn't already set. The check at the end of this method consults
        // `entity_scope_map` directly to decide whether to grant initial
        // authority to the former owner — overwriting an explicit exclude
        // would silently grant authority to a user who had been put
        // out-of-scope by the application (contract
        // [entity-delegation-09]: "migration yields no holder if owner is
        // out of scope at migration time").
        if self
            .send
            .state
            .entity_scope_map
            .get(user_key, global_entity)
            .is_none()
        {
            self.send
                .state
                .entity_scope_map
                .insert(user_key, global_entity, true);
        }

        // Migrate Entity from Remote -> Host connection
        let Some(user) = self.sim_handle.state.user_store.get(user_key) else {
            panic!("user should exist");
        };
        let Some(send_conn) = self
            .send
            .state
            .send_user_connections
            .get_mut(&user.address())
        else {
            panic!("connection does not exist")
        };

        // Step 0: Capture old RemoteEntity BEFORE migration (will be needed for MigrateResponse)
        let old_remote_entity = match send_conn
            .base
            .world_manager
            .entity_converter()
            .global_entity_to_remote_entity(global_entity)
        {
            Ok(entity) => entity,
            Err(error) => {
                panic!(
                    "Entity must exist as RemoteEntity before delegation: {global_entity:?}: {error:?}"
                );
            }
        };

        // Step 1: Migrate entity from RemoteEntity to HostEntity
        // This creates the HostEntity in HostEngine so it can receive commands
        let new_host_entity = match send_conn
            .base
            .world_manager
            .migrate_entity_remote_to_host(global_entity)
        {
            Ok(entity) => entity,
            Err(e) => {
                panic!("Failed to migrate entity during delegation: {e}");
            }
        };

        // Step 2: Force the server's HostEntityChannel into Delegated state locally
        // This allows MigrateResponse to be sent (requires Delegated state)
        // We do NOT send EnableDelegation back to the client - they already sent it!
        send_conn
            .base
            .world_manager
            .host_local_enable_delegation(new_host_entity);

        // Step 3: Send MigrateResponse to client
        // This will be the FIRST message in the new HostEntityChannel sequence (subcommand_id=0)
        send_conn.base.world_manager.host_send_migrate_response(
            global_entity,
            old_remote_entity,
            new_host_entity,
        );

        self.shared
            .global_world_manager
            .write()
            .entity_enable_delegation(global_entity);
        let entity_map = self.shared.global_entity_map.read();
        world.entity_enable_delegation(
            &self.shared.component_kinds,
            &*entity_map,
            &*self.shared.global_world_manager.read(),
            world_entity,
        );

        // Per contracts [entity-delegation-06]/[07]/[08]/[09]: the
        // previous owner gets initial Granted authority *iff* it's
        // still in-scope for the entity at migration time. If the
        // owner is out-of-scope, no holder is assigned and every
        // in-scope client observes Available (the default emitted by
        // EnableDelegation). We use `entity_scope_map` directly
        // because `user_scope_has_entity` takes a world_entity (E),
        // not a global_entity, and we only have the global here.
        let owner_in_scope = self
            .send
            .state
            .entity_scope_map
            .get(client_key, global_entity)
            .copied()
            .unwrap_or(false);

        if owner_in_scope {
            let requester = AuthOwner::from_user_key(Some(client_key));
            let result = self
                .shared
                .global_world_manager
                .write()
                .client_request_authority(global_entity, &requester);
            assert!(
                result.is_ok(),
                "failed to grant authority of client-owned delegated entity to creating user"
            );

            // Fan out SetAuthority to every in-scope user so the holder
            // observes Granted and everyone else observes Denied.
            // Without this, the per-client auth status stays at the
            // EnableDelegation default (Available) and contracts
            // [entity-delegation-06]/[entity-delegation-07] (migration
            // assigns initial authority to the previous owner) silently
            // fail. Snapshot first so we can re-borrow user_connections
            // mutably inside the loop.
            let user_snapshot: Vec<(UserKey, std::net::SocketAddr)> = self
                .sim_handle
                .state
                .user_store
                .iter()
                .map(|(k, u)| (*k, u.address()))
                .collect();
            for (user_key, addr) in user_snapshot {
                let Some(send_conn) = self.send.state.send_user_connections.get_mut(&addr) else {
                    continue;
                };
                if !send_conn
                    .base
                    .world_manager
                    .has_global_entity(global_entity)
                {
                    continue;
                }
                let new_status = if user_key == client_key {
                    EntityAuthStatus::Granted
                } else {
                    EntityAuthStatus::Denied
                };
                send_conn
                    .base
                    .world_manager
                    .host_send_set_auth(global_entity, new_status);
            }
        }
        // else: owner is out-of-scope — leave AuthOwner::None and don't
        // emit any SetAuthority. Every in-scope client already sees
        // Available from the EnableDelegation default.
    }

    pub(crate) fn entity_disable_delegation<W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        global_entity: GlobalEntity,
        world_entity: &E,
    ) {
        // TODO: check that entity is eligible for delegation?

        // for any users that have this entity in scope, send an `disable_delegation` message
        {
            // TODO: we can make this more efficient in the future by caching which Entities
            // are in each User's scope
            for (_user_key, user) in self.sim_handle.state.user_store.iter() {
                let Some(send_conn) = self
                    .send
                    .state
                    .send_user_connections
                    .get_mut(&user.address())
                else {
                    continue;
                };

                if !send_conn
                    .base
                    .world_manager
                    .has_global_entity(global_entity)
                {
                    // entity is not in scope for this connection
                    continue;
                }

                // Send DisableDelegationEntity action through EntityActionEvent system
                send_conn
                    .base
                    .world_manager
                    .send_disable_delegation(global_entity);
            }
        }

        self.shared
            .global_world_manager
            .write()
            .entity_disable_delegation(global_entity);
        world.entity_disable_delegation(world_entity);
    }

    //// Users

    /// Get a User's Socket Address, given the associated `UserKey`
    pub(crate) fn user_address(&self, user_key: UserKey) -> Option<SocketAddr> {
        self.sim_handle.user_address(user_key)
    }

    /// Returns an iterator of all the keys of the [`Room`]s the User belongs to
    pub(crate) fn user_room_keys(&'_ self, user_key: UserKey) -> Option<Iter<'_, RoomKey>> {
        self.sim_handle.state.user_store.room_keys_iter(user_key)
    }

    /// Get an count of how many Rooms the given User is inside
    pub(crate) fn user_rooms_count(&self, user_key: UserKey) -> Option<usize> {
        self.sim_handle.state.user_store.rooms_count(user_key)
    }

    pub(crate) fn user_disconnect<W: WorldMutType<E>>(
        &mut self,
        user_key: UserKey,
        reason: DisconnectReason,
        world: &mut W,
    ) {
        if self.shared.client_authoritative_entities {
            self.despawn_all_remote_entities(user_key, world);
            // 4-F.naia.a: clone the owned-entity set out from under the
            // gwm read guard so we can call `entity_release_authority`
            // (which takes `&mut self`) below without holding the lock.
            let copied_entities_opt: Option<Vec<GlobalEntity>> = self
                .shared
                .global_world_manager
                .read()
                .user_all_owned_entities(user_key)
                .map(|s| s.iter().copied().collect());
            if let Some(copied_entities) = copied_entities_opt {
                for global_entity in copied_entities {
                    // Only release authority if entity still exists (may have been despawned already).
                    // Resolve world_entity in a scoped read so the guard drops before
                    // the mutating `entity_release_authority` call.
                    let world_entity_opt = self
                        .shared
                        .global_entity_map
                        .read()
                        .global_entity_to_entity(global_entity)
                        .ok();
                    if let Some(world_entity) = world_entity_opt {
                        let _ = self.entity_release_authority(Some(user_key), &world_entity);
                    }
                }
            }
        }
        let user = self.user_delete(user_key);
        self.recv
            .state
            .incoming_world_events
            .push_disconnection(user_key, user.address(), reason);
    }

    pub(crate) fn user_queue_disconnect(&mut self, user_key: UserKey, reason: DisconnectReason) {
        let Some(user) = self.sim_handle.state.user_store.get(user_key) else {
            // User already disconnected, this is fine (disconnect packets may arrive multiple times)
            return;
        };
        let Some(recv_conn) = self
            .recv
            .state
            .recv_user_connections
            .get_mut(&user.address())
        else {
            // Connection already gone, user is being/has been disconnected
            return;
        };

        // If already marked for disconnect, don't queue again (idempotent)
        if recv_conn.manual_disconnect {
            return;
        }

        recv_conn.manual_disconnect = true;
        // Add to outstanding_disconnects immediately so it gets processed in the next process_all_packets call
        self.recv
            .state
            .outstanding_disconnects
            .push((user_key, reason));
    }

    pub(crate) fn user_delete(&mut self, user_key: UserKey) -> WorldUser {
        let Some(user) = self.sim_handle.state.user_store.remove(user_key) else {
            panic!("Attempting to delete non-existent user!");
        };

        let user_addr = user.address();

        info!("deleting authenticated user for {}", user.address());
        // 4-E.2d: drop recv-side directly (coordinator owns the recv map
        // in pipeline mode too).
        // 4-E.2e: send-side removal routes through the SendStateUpdate
        // queue so the coordinator never reaches into the send-side
        // map; drained inline below for same-cycle visibility.
        self.recv.state.recv_user_connections.remove(&user_addr);
        self.shared
            .pending_send_state_updates
            .lock()
            .push(crate::server::SendStateUpdate::ConnectionRemoved(user_addr));
        self.commit_pending_send_state_updates();

        // Drop this user's entire per-user priority layer so entries never
        // leak across user sessions.
        self.send.state.user_priorities.remove(&user_key);
        // nydus-dm9: drop the observer's evidence stream with it. A
        // reconnect starts a fresh stream (and a fresh generation).
        self.send.state.cause_streams.remove(&user_key);

        self.send.state.entity_scope_map.remove_user(user_key);

        // Clean up all user data
        for room_key in user.room_keys() {
            if let Some(room) = self.sim_handle.state.room_store.get_mut(*room_key) {
                room.unsubscribe_user(user_key);
            }
            // Mirror the room→user removal into the scope-checks cache —
            // this path bypasses `room_remove_user`.
            self.send
                .state
                .scope_checks_cache
                .on_user_removed_from_room(*room_key, user_key);
        }

        // remove from bandwidth monitor
        if self.send.state.send_io.bandwidth_monitor_enabled() {
            self.recv.state.recv_io.deregister_client(&user.address());
            self.send.state.send_io.deregister_client(&user.address());
        }

        self.sim_handle
            .state
            .global_request_manager
            .purge_user(user_key);
        self.sim_handle
            .state
            .global_response_manager
            .purge_user(user_key);

        user
    }

    /// All necessary cleanup, when they're actually gone...
    pub(crate) fn despawn_all_remote_entities<W: WorldMutType<E>>(
        &mut self,
        user_key: UserKey,
        world: &mut W,
    ) {
        let Some(user) = self.sim_handle.state.user_store.get(user_key) else {
            panic!("Attempting to despawn entities for a nonexistent user");
        };
        let Some(send_conn) = self
            .send
            .state
            .send_user_connections
            .get_mut(&user.address())
        else {
            panic!("Attempting to despawn entities on a nonexistent connection");
        };

        let remote_global_entities = send_conn.base.world_manager.remote_entities();
        let entity_events = {
            let entity_map = self.shared.global_entity_map.read();
            SharedGlobalWorldManager::despawn_all_entities(
                world,
                &*entity_map,
                &*self.shared.global_world_manager.read(),
                remote_global_entities,
                self.current_tick(),
            )
        };
        self.process_entity_events(world, user_key, entity_events);
    }

    //// Rooms

    /// Deletes the Room associated with a given `RoomKey` on the Server.
    /// Returns true if the Room existed.
    pub(crate) fn room_destroy(&mut self, room_key: RoomKey) -> bool {
        let existed = self.sim_handle.room_destroy(&room_key);
        self.send
            .state
            .apply_pending_room_changes(&self.shared.scope_change_queue);
        existed
    }

    //////// users

    /// Returns whether or not an User is currently in a specific Room, given
    /// their keys.
    pub(crate) fn room_has_user(&self, room_key: RoomKey, user_key: UserKey) -> bool {
        self.sim_handle.room_has_user(room_key, user_key)
    }

    /// Add an User to a Room, given the appropriate `RoomKey` & `UserKey`
    /// Entities will only ever be in-scope for Users which are in a
    /// Room with them
    pub(crate) fn room_add_user(&mut self, room_key: RoomKey, user_key: UserKey) {
        #[cfg(feature = "e2e_debug")]
        {
            SERVER_ROOM_MOVE_CALLED.fetch_add(1, Ordering::Relaxed);
        }
        self.sim_handle.room_add_user(room_key, user_key);
        self.send
            .state
            .apply_pending_room_changes(&self.shared.scope_change_queue);
    }

    /// Removes a User from a Room
    pub(crate) fn room_remove_user(&mut self, room_key: RoomKey, user_key: UserKey) {
        #[cfg(feature = "e2e_debug")]
        {
            SERVER_ROOM_MOVE_CALLED.fetch_add(1, Ordering::Relaxed);
        }
        self.sim_handle.room_remove_user(room_key, user_key);
        self.send
            .state
            .apply_pending_room_changes(&self.shared.scope_change_queue);
    }

    /// Get a count of Users in a given Room
    pub(crate) fn room_users_count(&self, room_key: RoomKey) -> usize {
        self.sim_handle.room_users_count(&room_key)
    }

    /// Returns an iterator of the [`UserKey`] for Users that belong in the Room
    pub(crate) fn room_user_keys(&self, room_key: RoomKey) -> impl Iterator<Item = &UserKey> {
        self.sim_handle.room_user_keys(&room_key)
    }

    pub(crate) fn room_entities(&self, room_key: RoomKey) -> impl Iterator<Item = &GlobalEntity> {
        self.sim_handle.state.room_store.entities_iter(room_key)
    }

    /// Sends a message to all connected users in a given Room using a given channel
    pub(crate) fn room_broadcast_message(
        &mut self,
        channel_kind: &ChannelKind,
        room_key: RoomKey,
        message_box: Box<dyn Message>,
    ) {
        // Wrap once in Arc so per-user clones are refcount increments, not heap allocs.
        let container = MessageContainer::new(message_box);
        let user_keys: Vec<UserKey> = self
            .sim_handle
            .state
            .room_store
            .user_keys_iter(room_key)
            .copied()
            .collect();
        for user_key in &user_keys {
            let _ = self.send_message_inner(*user_key, channel_kind, container.clone());
        }
    }

    //////// entities

    /// Returns whether or not an Entity is currently in a specific Room, given
    /// their keys.
    pub(crate) fn room_has_entity(&self, room_key: RoomKey, entity: GlobalEntity) -> bool {
        self.sim_handle
            .state
            .room_store
            .has_entity(room_key, entity)
    }

    /// Add an Entity to a Room associated with the given `RoomKey`.
    /// Entities will only ever be in-scope for Users which are in a Room with
    /// them.
    pub(crate) fn room_add_entity(&mut self, room_key: RoomKey, world_entity: &E) {
        self.sim_handle.room_add_entity(&room_key, world_entity);
        self.send
            .state
            .apply_pending_room_changes(&self.shared.scope_change_queue);
    }

    /// Remove an Entity from a Room, associated with the given `RoomKey`
    pub(crate) fn room_remove_entity(&mut self, room_key: RoomKey, world_entity: &E) {
        self.sim_handle.room_remove_entity(&room_key, world_entity);
        self.send
            .state
            .apply_pending_room_changes(&self.shared.scope_change_queue);
    }

    /// Get a count of Entities in a given Room
    pub(crate) fn room_entities_count(&self, room_key: RoomKey) -> usize {
        self.sim_handle.room_entities_count(&room_key)
    }
}
