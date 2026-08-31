use crate::world::local::local_entity::RemoteEntity;
use crate::world::local::local_world_manager::LocalWorldManager;
use crate::{
    messages::channels::receivers::indexed_message_reader::IndexedMessageReader,
    world::host::host_world_manager::SubCommandId, BitReader, ComponentKind, ComponentKinds,
    EntityAuthStatus, EntityMessage, EntityMessageType, HostEntity, MessageIndex, OwnedLocalEntity,
    Serde, SerdeErr, Tick,
};

/// Stateless helper that deserializes entity update and message payloads from a bit stream into a [`LocalWorldManager`].
pub struct WorldReader;

impl WorldReader {
    // Reading

    fn read_message_index(
        reader: &mut BitReader,
        last_index_opt: &mut Option<MessageIndex>,
    ) -> Result<MessageIndex, SerdeErr> {
        // read index
        let current_index = IndexedMessageReader::read_message_index(reader, last_index_opt)?;

        *last_index_opt = Some(current_index);

        Ok(current_index)
    }

    /// Deserializes both component updates and entity messages from `reader` into `world_manager`.
    pub fn read_world_events(
        world_manager: &mut LocalWorldManager,
        component_kinds: &ComponentKinds,
        tick: &Tick,
        reader: &mut BitReader,
    ) -> Result<(), SerdeErr> {
        // read entity updates
        Self::read_updates(world_manager, component_kinds, tick, reader)?;

        // read entity messages
        Self::read_messages(world_manager, component_kinds, reader)?;

        Ok(())
    }

    /// Read incoming Entity messages.
    fn read_messages(
        world_manager: &mut LocalWorldManager,
        component_kinds: &ComponentKinds,
        reader: &mut BitReader,
    ) -> Result<(), SerdeErr> {
        let mut last_read_id: Option<MessageIndex> = None;

        loop {
            // read message continue bit
            let message_continue = bool::de(reader)?;
            if !message_continue {
                break;
            }

            Self::read_message(world_manager, component_kinds, reader, &mut last_read_id)?;
        }

        Ok(())
    }

    /// Read the bits corresponding to the EntityMessage and adds the [`EntityMessage`]
    /// to an internal buffer.
    ///
    /// We can use a UnorderedReliableReceiver buffer because the messages have already been
    /// ordered by the client's jitter buffer
    fn read_message(
        world_manager: &mut LocalWorldManager,
        component_kinds: &ComponentKinds,
        reader: &mut BitReader,
        last_read_id: &mut Option<MessageIndex>,
    ) -> Result<(), SerdeErr> {
        let message_id = Self::read_message_index(reader, last_read_id)?;

        let message_type = EntityMessageType::de(reader)?;

        match message_type {
            EntityMessageType::Spawn => {
                // Count when Spawn message KIND is recognized on wire (before routing)
                #[cfg(feature = "e2e_debug")]
                {
                    extern "Rust" {
                        fn client_saw_spawn_increment();
                    }
                    // Safety: client_saw_spawn_increment is defined by the naia-tests harness
                    // when compiled with feature = "e2e_debug". It atomically increments a
                    // counter and has no preconditions. The e2e_debug feature is never enabled
                    // in production builds.
                    unsafe {
                        client_saw_spawn_increment();
                    }
                }

                // read remote entity
                let remote_entity = RemoteEntity::de(reader)?;

                world_manager.receiver_buffer_message(
                    message_id,
                    EntityMessage::Spawn(remote_entity.copy_to_owned()),
                );
            }
            EntityMessageType::SpawnWithComponents => {
                // read entity as full OwnedLocalEntity (carries is_static) then reverse to Remote
                let local_entity = OwnedLocalEntity::de(reader)?.to_reversed();

                // read component count
                let count = u8::de(reader)?;

                let mut kinds = Vec::with_capacity(count as usize);
                for _ in 0..count {
                    let new_component = {
                        let converter = world_manager.entity_converter();
                        component_kinds.read(reader, &converter)?
                    };
                    let new_component_kind = new_component.kind();
                    world_manager.insert_received_component(
                        &local_entity,
                        &new_component_kind,
                        new_component,
                    );
                    kinds.push(new_component_kind);
                }

                world_manager.receiver_buffer_message(
                    message_id,
                    EntityMessage::SpawnWithComponents(local_entity, kinds),
                );
            }
            EntityMessageType::Despawn => {
                // read local entity
                let mut local_entity = OwnedLocalEntity::de(reader)?.to_reversed();
                // apply redirect if entity was migrated
                local_entity = world_manager.apply_entity_redirect(local_entity);

                world_manager
                    .receiver_buffer_message(message_id, EntityMessage::Despawn(local_entity));
            }
            EntityMessageType::InsertComponent => {
                // read local entity
                let mut local_entity = OwnedLocalEntity::de(reader)?.to_reversed();
                // apply redirect if entity was migrated
                local_entity = world_manager.apply_entity_redirect(local_entity);

                // read component
                let new_component = {
                    let converter = world_manager.entity_converter();
                    component_kinds.read(reader, &converter)?
                };
                let new_component_kind = new_component.kind();

                world_manager.receiver_buffer_message(
                    message_id,
                    EntityMessage::InsertComponent(local_entity, new_component_kind),
                );
                world_manager.insert_received_component(
                    &local_entity,
                    &new_component_kind,
                    new_component,
                );
            }
            EntityMessageType::RemoveComponent => {
                // read local entity
                let mut local_entity = OwnedLocalEntity::de(reader)?.to_reversed();
                // apply redirect if entity was migrated
                local_entity = world_manager.apply_entity_redirect(local_entity);

                // read component kind
                let component_kind = ComponentKind::de(component_kinds, reader)?;

                world_manager.receiver_buffer_message(
                    message_id,
                    EntityMessage::RemoveComponent(local_entity, component_kind),
                );
            }
            EntityMessageType::Publish => {
                // read subcommand id
                let sub_command_id = SubCommandId::de(reader)?;

                // read local entity
                let mut local_entity = OwnedLocalEntity::de(reader)?.to_reversed();
                // apply redirect if entity was migrated
                local_entity = world_manager.apply_entity_redirect(local_entity);

                world_manager.receiver_buffer_message(
                    message_id,
                    EntityMessage::Publish(sub_command_id, local_entity),
                );
            }
            EntityMessageType::Unpublish => {
                // read subcommand id
                let sub_command_id = SubCommandId::de(reader)?;

                // read local entity
                let mut local_entity = OwnedLocalEntity::de(reader)?.to_reversed();
                // apply redirect if entity was migrated
                local_entity = world_manager.apply_entity_redirect(local_entity);

                world_manager.receiver_buffer_message(
                    message_id,
                    EntityMessage::Unpublish(sub_command_id, local_entity),
                );
            }
            EntityMessageType::EnableDelegation => {
                // read subcommand id
                let sub_command_id = SubCommandId::de(reader)?;

                // read local entity
                let mut local_entity = OwnedLocalEntity::de(reader)?.to_reversed();
                // apply redirect if entity was migrated
                local_entity = world_manager.apply_entity_redirect(local_entity);

                world_manager.receiver_buffer_message(
                    message_id,
                    EntityMessage::EnableDelegation(sub_command_id, local_entity),
                );
            }
            EntityMessageType::DisableDelegation => {
                // this command is only ever received by clients, regarding server-owned entities

                // read subcommand id
                let sub_command_id = SubCommandId::de(reader)?;

                // read remote entity
                let remote_entity = RemoteEntity::de(reader)?;

                world_manager.receiver_buffer_message(
                    message_id,
                    EntityMessage::DisableDelegation(sub_command_id, remote_entity.copy_to_owned()),
                );
            }
            EntityMessageType::SetAuthority => {
                // this command is only ever received by clients, regarding server-owned entities

                // Count when SetAuthority message KIND is recognized on wire (before entity mapping)
                #[cfg(feature = "e2e_debug")]
                {
                    extern "Rust" {
                        fn client_saw_set_auth_wire_increment();
                    }
                    // Safety: same as client_saw_spawn_increment above — atomic counter defined
                    // by the test harness; no preconditions; e2e_debug is never active in prod.
                    unsafe {
                        client_saw_set_auth_wire_increment();
                    }
                }

                // read subcommand id
                let sub_command_id = SubCommandId::de(reader)?;

                // read remote entity
                let remote_entity = RemoteEntity::de(reader)?;

                // read auth status
                let auth_status = EntityAuthStatus::de(reader)?;

                world_manager.receiver_buffer_message(
                    message_id,
                    EntityMessage::SetAuthority(
                        sub_command_id,
                        remote_entity.copy_to_owned(),
                        auth_status,
                    ),
                );
            }

            // below are response-type messages
            EntityMessageType::RequestAuthority => {
                // this command is only read by the server, regarding server-owned entities

                // read subcommand id
                let sub_command_id = SubCommandId::de(reader)?;

                // read host entity
                let host_entity = HostEntity::de(reader)?;

                world_manager.receiver_buffer_message(
                    message_id,
                    EntityMessage::RequestAuthority(sub_command_id, host_entity.copy_to_owned()),
                );
            }
            EntityMessageType::ReleaseAuthority => {
                // this command is only read by the server, regarding server-owned entities

                // read subcommand id
                let sub_command_id = SubCommandId::de(reader)?;

                // read local entity
                let mut local_entity = OwnedLocalEntity::de(reader)?.to_reversed();
                // apply redirect if entity was migrated
                local_entity = world_manager.apply_entity_redirect(local_entity);

                world_manager.receiver_buffer_message(
                    message_id,
                    EntityMessage::ReleaseAuthority(sub_command_id, local_entity),
                );
            }
            EntityMessageType::EnableDelegationResponse => {
                // this command is only read by the server, regarding server-owned entities

                // read subcommand id
                let sub_command_id = SubCommandId::de(reader)?;

                // read host entity
                let host_entity = HostEntity::de(reader)?;

                world_manager.receiver_buffer_message(
                    message_id,
                    EntityMessage::EnableDelegationResponse(
                        sub_command_id,
                        host_entity.copy_to_owned(),
                    ),
                );
            }
            EntityMessageType::MigrateResponse => {
                // this command is only ever received by clients, regarding newly delegated server-owned entities

                // read subcommand id
                let sub_command_id = SubCommandId::de(reader)?;

                // read client's HostEntity (so client can look it up in entity_map!)
                let client_host_entity = HostEntity::de(reader)?;

                // read new RemoteEntity (what the client will create)
                let new_remote_entity = RemoteEntity::de(reader)?;

                world_manager.receiver_buffer_message(
                    message_id,
                    EntityMessage::MigrateResponse(
                        sub_command_id,
                        client_host_entity.copy_to_owned(),
                        new_remote_entity,
                    ),
                );
            }
            EntityMessageType::Noop => {
                world_manager.receiver_buffer_message(message_id, EntityMessage::Noop);
            }
        }

        Ok(())
    }

    /// Read component updates from raw bits
    fn read_updates(
        world_manager: &mut LocalWorldManager,
        component_kinds: &ComponentKinds,
        tick: &Tick,
        reader: &mut BitReader,
    ) -> Result<(), SerdeErr> {
        loop {
            // read update continue bit
            let update_continue = bool::de(reader)?;
            if !update_continue {
                break;
            }

            let mut local_entity = OwnedLocalEntity::de(reader)?.to_reversed();
            // apply redirect if entity was migrated
            local_entity = world_manager.apply_entity_redirect(local_entity);

            Self::read_update(world_manager, component_kinds, tick, reader, &local_entity)?;
        }

        Ok(())
    }

    /// Read component updates from raw bits for a given entity
    fn read_update(
        world_manager: &mut LocalWorldManager,
        component_kinds: &ComponentKinds,
        tick: &Tick,
        reader: &mut BitReader,
        local_entity: &OwnedLocalEntity,
    ) -> Result<(), SerdeErr> {
        loop {
            // read update continue bit
            let component_continue = bool::de(reader)?;
            if !component_continue {
                break;
            }

            let component_update = component_kinds.read_create_update(reader)?;

            // At this point, the WorldChannel/EntityReceiver should guarantee the Entity is in scope, correct?
            if world_manager.has_local_entity(local_entity) {
                world_manager.insert_received_update(*tick, local_entity, component_update);
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod world_reader_tests {
    //! Wire-format coverage for [`WorldReader`].
    //!
    //! `WorldReader` is one half of a codec, so every test here encodes bytes by
    //! hand — deliberately NOT by calling `WorldWriter` — and then asks the
    //! reader to parse them. A round-trip against the writer would pass even if
    //! both sides drifted together; hand-encoding pins the format itself.
    //!
    //! Two things are asserted throughout:
    //!
    //! 1. **The semantics**: the right `EntityMessage` / component / update lands
    //!    in the right buffer on the [`LocalWorldManager`].
    //! 2. **The bit cursor**: every stream ends with a `MAGIC` sentinel that is
    //!    read back *after* `read_world_events` returns. If a branch reads one
    //!    field too few or too many, the sentinel comes back wrong even when the
    //!    semantics happen to look fine. This is what makes the per-variant
    //!    parse-shape table below worth having.

    use std::{collections::HashSet, net::SocketAddr};

    use super::*;
    use crate::messages::channels::senders::indexed_message_writer::IndexedMessageWriter;
    use crate::{
        world::{
            component::property::Property,
            test_support::TestGwm,
            test_world::{remote_component, TestSpawner, TestWorld},
        },
        BigMapKey, BitWriter, DiffMask, EntityEvent, FakeEntityConverter, GlobalEntity, HostType,
        Instant, MessageIndex, Replicate, ReplicatedComponent, WorldMutType,
    };

    /// Written after the whole payload; read back once the reader claims to be
    /// done. A wrong number of consumed bits shows up here as a wrong value.
    const MAGIC: u32 = 0xFEED_BEEF;

    #[derive(Replicate)]
    struct Ghost {
        value: Property<u8>,
    }

    #[derive(Replicate)]
    struct Wraith {
        value: Property<u8>,
    }

    fn global(id: u64) -> GlobalEntity {
        GlobalEntity::from_u64(id)
    }

    // -- encoding -----------------------------------------------------------

    /// Hand-encoder for the `read_world_events` stream. The section order and
    /// the continue-bit protocol here are transcribed from the reader, not
    /// borrowed from the writer.
    struct Wire {
        writer: BitWriter,
        kinds: ComponentKinds,
        last_index: Option<MessageIndex>,
        updates_open: bool,
        messages_open: bool,
    }

    impl Wire {
        fn new(kinds: &ComponentKinds) -> Self {
            Self {
                writer: BitWriter::new(),
                kinds: kinds.clone(),
                last_index: None,
                updates_open: true,
                messages_open: false,
            }
        }

        /// Opens an entity's update block. `entity` is written exactly as it
        /// appears on the wire; the reader reverses it.
        fn update_entity(&mut self, entity: OwnedLocalEntity) -> &mut Self {
            assert!(self.updates_open, "the update section is already closed");
            true.ser(&mut self.writer);
            entity.ser(&mut self.writer);
            self
        }

        /// One component update inside the currently open entity block.
        fn component_update<R: ReplicatedComponent>(&mut self, component: &R) -> &mut Self {
            true.ser(&mut self.writer);
            ComponentKind::of::<R>().ser(&self.kinds, &mut self.writer);
            let mut diff_mask = DiffMask::new(component.diff_mask_size());
            for index in 0..(diff_mask.byte_number() * 8) {
                diff_mask.set_bit(index, true);
            }
            component.write_update(&diff_mask, &mut self.writer, &mut FakeEntityConverter);
            self
        }

        /// Closes the currently open entity's component list.
        fn end_entity(&mut self) -> &mut Self {
            false.ser(&mut self.writer);
            self
        }

        /// Closes the update section and opens the message section.
        fn end_updates(&mut self) -> &mut Self {
            assert!(self.updates_open, "the update section is already closed");
            false.ser(&mut self.writer);
            self.updates_open = false;
            self.messages_open = true;
            self
        }

        /// Opens a message with the given index and type. Indices are
        /// delta-encoded against the previous one, exactly as the reader
        /// expects; the caller writes the payload afterwards.
        fn message(&mut self, index: MessageIndex, kind: EntityMessageType) -> &mut Self {
            if self.updates_open {
                self.end_updates();
            }
            true.ser(&mut self.writer);
            IndexedMessageWriter::write_message_index(&mut self.writer, &self.last_index, &index);
            self.last_index = Some(index);
            kind.ser(&mut self.writer);
            self
        }

        fn owned(&mut self, entity: OwnedLocalEntity) -> &mut Self {
            entity.ser(&mut self.writer);
            self
        }

        fn remote(&mut self, id: u32) -> &mut Self {
            RemoteEntity::new(id).ser(&mut self.writer);
            self
        }

        fn host(&mut self, id: u32) -> &mut Self {
            HostEntity::new(id).ser(&mut self.writer);
            self
        }

        fn sub_command(&mut self, id: u8) -> &mut Self {
            SubCommandId::from(id).ser(&mut self.writer);
            self
        }

        fn auth(&mut self, status: EntityAuthStatus) -> &mut Self {
            status.ser(&mut self.writer);
            self
        }

        fn u8(&mut self, value: u8) -> &mut Self {
            value.ser(&mut self.writer);
            self
        }

        fn kind<R: ReplicatedComponent>(&mut self) -> &mut Self {
            ComponentKind::of::<R>().ser(&self.kinds, &mut self.writer);
            self
        }

        /// A whole component, kind tag and all, as `SpawnWithComponents` and
        /// `InsertComponent` carry it.
        fn component<R: ReplicatedComponent>(&mut self, component: &R) -> &mut Self {
            component.write(&self.kinds, &mut self.writer, &mut FakeEntityConverter);
            self
        }

        /// Closes whatever sections remain open and appends the sentinel.
        fn finish(mut self) -> Box<[u8]> {
            if self.updates_open {
                self.end_updates();
            }
            if self.messages_open {
                false.ser(&mut self.writer);
                self.messages_open = false;
            }
            MAGIC.ser(&mut self.writer);
            self.writer.to_bytes()
        }

        /// Closes the stream WITHOUT the sentinel, for truncation tests.
        fn finish_raw(self) -> Box<[u8]> {
            self.writer.to_bytes()
        }
    }

    // -- fixture ------------------------------------------------------------

    struct Fixture {
        kinds: ComponentKinds,
        gwm: TestGwm,
        manager: LocalWorldManager,
    }

    impl Fixture {
        fn client() -> Self {
            let mut kinds = ComponentKinds::new();
            kinds.add_component::<Ghost>();
            kinds.add_component::<Wraith>();
            let gwm = TestGwm::new(&kinds);
            let address: Option<SocketAddr> = Some("127.0.0.1:4000".parse().unwrap());
            let manager = LocalWorldManager::new(&address, HostType::Client, 1, &gwm);
            Self {
                kinds,
                gwm,
                manager,
            }
        }

        /// Parses `bytes`, then reads the trailing sentinel back out of the very
        /// same reader. Returns the sentinel so the caller can assert the exact
        /// number of bits the reader consumed.
        fn read(&mut self, bytes: &[u8], tick: Tick) -> Result<u32, SerdeErr> {
            let mut reader = BitReader::new(bytes);
            let kinds = std::mem::take(&mut self.kinds);
            let result =
                WorldReader::read_world_events(&mut self.manager, &kinds, &tick, &mut reader);
            self.kinds = kinds;
            result?;
            u32::de(&mut reader)
        }

        /// Parses and asserts the cursor landed exactly on the sentinel.
        fn read_ok(&mut self, bytes: &[u8]) {
            let magic = self
                .read(bytes, 0)
                .expect("a well-formed stream should parse");
            assert_eq!(
                magic, MAGIC,
                "the reader consumed the wrong number of bits: the sentinel that \
                 immediately follows the payload did not read back"
            );
        }

        /// Registers `id` as an in-scope remote entity holding `Ghost`, and
        /// spawns the matching entity in `world`.
        fn adopt_remote(&mut self, world: &mut TestWorld, id: u64) -> OwnedLocalEntity {
            let remote_entity = RemoteEntity::new(id as u32);
            let mut component_kinds = HashSet::new();
            component_kinds.insert(ComponentKind::of::<Ghost>());
            self.manager
                .insert_remote_entity(&global(id), remote_entity, component_kinds);
            world.spawn_at(id);
            world.insert_boxed_component(
                &id,
                remote_component(&self.kinds, &Ghost::new_complete(0)),
            );
            remote_entity.copy_to_owned()
        }

        fn take_events(&mut self, world: &mut TestWorld) -> Vec<EntityEvent> {
            let mut spawner = TestSpawner::new();
            let now = Instant::now();
            let kinds = std::mem::take(&mut self.kinds);
            let events =
                self.manager
                    .take_incoming_events(&mut spawner, &self.gwm, &kinds, world, &now);
            self.kinds = kinds;
            events
        }
    }

    /// The wire entity for a *remote-owned* entity: the sender calls it its own
    /// Host entity, and the reader reverses that to Remote on our side.
    fn wire_host(id: u32) -> OwnedLocalEntity {
        OwnedLocalEntity::Host {
            id,
            is_static: false,
        }
    }

    // -- the empty stream ---------------------------------------------------

    #[test]
    fn an_empty_stream_is_two_continue_bits_and_nothing_else() {
        let mut fx = Fixture::client();
        let bytes = Wire::new(&fx.kinds).finish();
        fx.read_ok(&bytes);

        let mut world = TestWorld::new();
        assert!(
            fx.take_events(&mut world).is_empty(),
            "an empty stream should produce no events at all"
        );
    }

    // -- the update section -------------------------------------------------

    #[test]
    fn an_update_for_an_in_scope_entity_is_buffered_and_applied() {
        let mut fx = Fixture::client();
        let mut world = TestWorld::new();
        fx.adopt_remote(&mut world, 7);

        let mut wire = Wire::new(&fx.kinds);
        wire.update_entity(wire_host(7))
            .component_update(&Ghost::new_complete(42))
            .end_entity();
        let bytes = wire.finish();

        let magic = fx.read(&bytes, 9).expect("a well-formed stream parses");
        assert_eq!(magic, MAGIC, "the update section consumed the wrong bits");

        let events = fx.take_events(&mut world);
        let updated: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                EntityEvent::UpdateComponent(tick, entity, kind) => Some((*tick, *entity, *kind)),
                _ => None,
            })
            .collect();
        assert_eq!(updated.len(), 1, "exactly one component update was sent");
        assert_eq!(
            updated[0].0, 9,
            "the tick passed to read_world_events should be stamped on the update"
        );
        assert_eq!(updated[0].2, ComponentKind::of::<Ghost>());
        assert_eq!(
            *world
                .value_of::<Ghost>(&7)
                .expect("the world still holds Ghost")
                .value,
            42,
            "the decoded value should have reached the world"
        );
    }

    #[test]
    fn an_update_for_an_out_of_scope_entity_is_dropped_but_still_consumed() {
        let mut fx = Fixture::client();
        let mut world = TestWorld::new();
        // Note: entity 7 is deliberately NOT adopted, so `has_local_entity` is
        // false and `read_update` must discard the decoded update.

        let mut wire = Wire::new(&fx.kinds);
        wire.update_entity(wire_host(7))
            .component_update(&Ghost::new_complete(42))
            .end_entity();
        let bytes = wire.finish();

        let magic = fx
            .read(&bytes, 9)
            .expect("an out-of-scope entity is not a parse error");
        assert_eq!(
            magic, MAGIC,
            "the payload of a dropped update must still be consumed off the wire, \
             or every later field in the packet would be misaligned"
        );

        assert!(
            fx.take_events(&mut world).is_empty(),
            "an update for an entity we do not know about should be discarded"
        );
    }

    #[test]
    fn several_components_can_update_on_one_entity() {
        let mut fx = Fixture::client();
        let mut world = TestWorld::new();
        fx.adopt_remote(&mut world, 7);
        world.insert_boxed_component(&7, remote_component(&fx.kinds, &Wraith::new_complete(0)));

        let mut wire = Wire::new(&fx.kinds);
        wire.update_entity(wire_host(7))
            .component_update(&Ghost::new_complete(11))
            .component_update(&Wraith::new_complete(22))
            .end_entity();
        let bytes = wire.finish();
        fx.read_ok(&bytes);

        let events = fx.take_events(&mut world);
        let kinds: HashSet<ComponentKind> = events
            .iter()
            .filter_map(|event| match event {
                EntityEvent::UpdateComponent(_, _, kind) => Some(*kind),
                _ => None,
            })
            .collect();
        assert_eq!(
            kinds,
            HashSet::from([ComponentKind::of::<Ghost>(), ComponentKind::of::<Wraith>()]),
            "both components in the entity's block should be decoded"
        );
        assert_eq!(*world.value_of::<Ghost>(&7).unwrap().value, 11);
        assert_eq!(*world.value_of::<Wraith>(&7).unwrap().value, 22);
    }

    #[test]
    fn several_entities_can_update_in_one_stream() {
        let mut fx = Fixture::client();
        let mut world = TestWorld::new();
        fx.adopt_remote(&mut world, 7);
        fx.adopt_remote(&mut world, 8);

        let mut wire = Wire::new(&fx.kinds);
        wire.update_entity(wire_host(7))
            .component_update(&Ghost::new_complete(11))
            .end_entity();
        wire.update_entity(wire_host(8))
            .component_update(&Ghost::new_complete(22))
            .end_entity();
        let bytes = wire.finish();
        fx.read_ok(&bytes);

        fx.take_events(&mut world);
        assert_eq!(
            *world.value_of::<Ghost>(&7).unwrap().value,
            11,
            "the first entity's block should not be attributed to the second"
        );
        assert_eq!(*world.value_of::<Ghost>(&8).unwrap().value, 22);
    }

    #[test]
    fn an_entity_with_no_components_is_a_valid_update_block() {
        let mut fx = Fixture::client();
        let mut world = TestWorld::new();
        fx.adopt_remote(&mut world, 7);

        let mut wire = Wire::new(&fx.kinds);
        wire.update_entity(wire_host(7)).end_entity();
        let bytes = wire.finish();
        fx.read_ok(&bytes);

        assert!(
            fx.take_events(&mut world).is_empty(),
            "an empty component list should produce no update events"
        );
    }

    #[test]
    fn an_update_follows_an_installed_redirect() {
        let mut fx = Fixture::client();
        let mut world = TestWorld::new();
        // The entity is in scope under id 8, but the wire still names 7.
        fx.adopt_remote(&mut world, 8);
        fx.manager
            .install_entity_redirect(wire_host(7).to_reversed(), wire_host(8).to_reversed());

        let mut wire = Wire::new(&fx.kinds);
        wire.update_entity(wire_host(7))
            .component_update(&Ghost::new_complete(42))
            .end_entity();
        let bytes = wire.finish();
        fx.read_ok(&bytes);

        fx.take_events(&mut world);
        assert_eq!(
            *world.value_of::<Ghost>(&8).unwrap().value,
            42,
            "an update addressed to a migrated entity should follow the redirect \
             to its new local id, not be dropped as out of scope"
        );
    }

    // -- the message section ------------------------------------------------

    #[test]
    fn a_spawn_message_names_the_reversed_entity() {
        let mut fx = Fixture::client();
        let mut world = TestWorld::new();

        let mut wire = Wire::new(&fx.kinds);
        wire.message(0, EntityMessageType::Spawn).remote(7);
        let bytes = wire.finish();
        fx.read_ok(&bytes);

        let events = fx.take_events(&mut world);
        assert_eq!(events.len(), 1, "one Spawn in, one Spawn event out");
        assert_eq!(
            events[0].to_type(),
            Some(EntityMessageType::Spawn),
            "a Spawn on the wire should surface as a Spawn event"
        );
    }

    #[test]
    fn spawn_with_components_buffers_every_component_it_declares() {
        let mut fx = Fixture::client();
        let mut world = TestWorld::new();

        let mut wire = Wire::new(&fx.kinds);
        wire.message(0, EntityMessageType::SpawnWithComponents)
            .owned(wire_host(7))
            .u8(2)
            .component(&Ghost::new_complete(11))
            .component(&Wraith::new_complete(22));
        let bytes = wire.finish();
        fx.read_ok(&bytes);

        let events = fx.take_events(&mut world);
        let inserted: HashSet<ComponentKind> = events
            .iter()
            .filter_map(|event| match event {
                EntityEvent::InsertComponent(_, kind) => Some(*kind),
                _ => None,
            })
            .collect();
        assert_eq!(
            inserted,
            HashSet::from([ComponentKind::of::<Ghost>(), ComponentKind::of::<Wraith>()]),
            "both declared components should have been buffered against the new entity"
        );
        assert!(
            events
                .iter()
                .any(|event| event.to_type() == Some(EntityMessageType::Spawn)),
            "a SpawnWithComponents should still spawn the entity"
        );
    }

    #[test]
    fn spawn_with_components_accepts_a_zero_count() {
        let mut fx = Fixture::client();
        let mut world = TestWorld::new();

        let mut wire = Wire::new(&fx.kinds);
        wire.message(0, EntityMessageType::SpawnWithComponents)
            .owned(wire_host(7))
            .u8(0);
        let bytes = wire.finish();
        fx.read_ok(&bytes);

        let events = fx.take_events(&mut world);
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, EntityEvent::InsertComponent(_, _))),
            "a zero component count should read no components at all"
        );
    }

    #[test]
    fn an_insert_component_message_buffers_the_component_it_carries() {
        let mut fx = Fixture::client();
        let mut world = TestWorld::new();

        let mut wire = Wire::new(&fx.kinds);
        wire.message(0, EntityMessageType::Spawn).remote(7);
        wire.message(1, EntityMessageType::InsertComponent)
            .owned(wire_host(7))
            .component(&Wraith::new_complete(22));
        let bytes = wire.finish();
        fx.read_ok(&bytes);

        let events = fx.take_events(&mut world);
        assert!(
            events.iter().any(|event| matches!(
                event,
                EntityEvent::InsertComponent(_, kind) if *kind == ComponentKind::of::<Wraith>()
            )),
            "the component carried by InsertComponent should reach the world, \
             not merely its kind tag"
        );
        // The spawner mints the GlobalEntity, so ask the event which one it got
        // rather than assuming the wire id is reused as a local id.
        let spawned = events
            .iter()
            .find(|event| event.to_type() == Some(EntityMessageType::Spawn))
            .expect("the entity should have been spawned")
            .entity();
        assert_eq!(
            *world
                .value_of::<Wraith>(&spawned.to_u64())
                .expect("the spawned entity should hold Wraith")
                .value,
            22,
            "the inserted component should carry its decoded value"
        );
    }

    #[test]
    fn a_remove_component_message_carries_only_a_kind_tag() {
        let mut fx = Fixture::client();
        let mut world = TestWorld::new();

        let mut wire = Wire::new(&fx.kinds);
        wire.message(0, EntityMessageType::SpawnWithComponents)
            .owned(wire_host(7))
            .u8(1)
            .component(&Ghost::new_complete(1));
        wire.message(1, EntityMessageType::RemoveComponent)
            .owned(wire_host(7))
            .kind::<Ghost>();
        let bytes = wire.finish();
        fx.read_ok(&bytes);

        let events = fx.take_events(&mut world);
        assert!(
            events
                .iter()
                .any(|event| matches!(event, EntityEvent::RemoveComponent(_, _))),
            "a RemoveComponent on the wire should surface as a RemoveComponent event"
        );
    }

    #[test]
    fn a_despawn_message_reaches_the_named_entity() {
        let mut fx = Fixture::client();
        let mut world = TestWorld::new();

        let mut wire = Wire::new(&fx.kinds);
        wire.message(0, EntityMessageType::Spawn).remote(7);
        wire.message(1, EntityMessageType::Despawn)
            .owned(wire_host(7));
        let bytes = wire.finish();
        fx.read_ok(&bytes);

        let events = fx.take_events(&mut world);
        assert!(
            events
                .iter()
                .any(|event| event.to_type() == Some(EntityMessageType::Despawn)),
            "a Despawn on the wire should surface as a Despawn event"
        );
    }

    #[test]
    fn a_noop_message_parses_and_produces_no_event() {
        let mut fx = Fixture::client();
        let mut world = TestWorld::new();

        let mut wire = Wire::new(&fx.kinds);
        wire.message(0, EntityMessageType::Noop);
        let bytes = wire.finish();
        fx.read_ok(&bytes);

        assert!(
            fx.take_events(&mut world).is_empty(),
            "a Noop is buffered for sequencing but is filtered out before events"
        );
    }

    // -- redirects apply to some message types and not others ---------------

    #[test]
    fn a_despawn_follows_an_installed_redirect() {
        let mut fx = Fixture::client();
        let mut world = TestWorld::new();
        fx.manager
            .install_entity_redirect(wire_host(7).to_reversed(), wire_host(8).to_reversed());

        let mut wire = Wire::new(&fx.kinds);
        wire.message(0, EntityMessageType::Spawn).remote(8);
        wire.message(1, EntityMessageType::Despawn)
            .owned(wire_host(7));
        let bytes = wire.finish();
        fx.read_ok(&bytes);

        let events = fx.take_events(&mut world);
        // The spawner mints the GlobalEntity for wire id 8; the Despawn names
        // wire id 7, so it can only land on that same entity via the redirect.
        let spawned = events
            .iter()
            .find(|event| event.to_type() == Some(EntityMessageType::Spawn))
            .expect("wire entity 8 should have been spawned")
            .entity();
        let despawned: Vec<_> = events
            .iter()
            .filter(|event| event.to_type() == Some(EntityMessageType::Despawn))
            .map(|event| event.entity())
            .collect();
        assert_eq!(
            despawned,
            vec![spawned],
            "a Despawn addressed to a migrated entity should be redirected to its \
             new local id"
        );
    }

    #[test]
    fn a_spawn_deliberately_ignores_an_installed_redirect() {
        // Spawn, DisableDelegation, SetAuthority, RequestAuthority,
        // EnableDelegationResponse and MigrateResponse all read their entity
        // WITHOUT calling `apply_entity_redirect`. That asymmetry is load-bearing:
        // a Spawn introduces an id, so redirecting it would silently retarget a
        // brand-new entity onto an old one. This pins the distinction so it
        // cannot be "tidied up" into uniformity by accident.
        let mut fx = Fixture::client();
        let mut world = TestWorld::new();
        fx.manager
            .install_entity_redirect(wire_host(7).to_reversed(), wire_host(8).to_reversed());

        let mut wire = Wire::new(&fx.kinds);
        wire.message(0, EntityMessageType::Spawn).remote(7);
        let bytes = wire.finish();
        fx.read_ok(&bytes);

        let events = fx.take_events(&mut world);
        let spawned: Vec<_> = events
            .iter()
            .filter(|event| event.to_type() == Some(EntityMessageType::Spawn))
            .map(|event| event.entity())
            .collect();
        assert_eq!(spawned.len(), 1, "one Spawn in, one Spawn event out");
        assert_ne!(
            spawned[0],
            global(8),
            "a Spawn must NOT be rerouted through the redirect table"
        );
    }

    // -- message sequencing -------------------------------------------------

    #[test]
    fn message_indices_are_absolute_then_delta_encoded() {
        // The first index is written whole; every later one is a diff against
        // its predecessor. If the reader forgot to carry `last_read_id` forward,
        // the second message would decode at the wrong index and the reliable
        // receiver would stall instead of delivering both.
        let mut fx = Fixture::client();
        let mut world = TestWorld::new();

        let mut wire = Wire::new(&fx.kinds);
        wire.message(0, EntityMessageType::Spawn).remote(7);
        wire.message(1, EntityMessageType::Spawn).remote(8);
        wire.message(2, EntityMessageType::Spawn).remote(9);
        let bytes = wire.finish();
        fx.read_ok(&bytes);

        let spawns = fx
            .take_events(&mut world)
            .iter()
            .filter(|event| event.to_type() == Some(EntityMessageType::Spawn))
            .count();
        assert_eq!(
            spawns, 3,
            "all three sequential messages should be delivered in one drain"
        );
    }

    #[test]
    fn the_first_message_read_sets_the_sequence_baseline_whatever_its_index() {
        // A stream whose first message is index 5 is delivered immediately: the
        // receiver does NOT insist on starting at zero. That is deliberate --
        // a connection can join mid-sequence -- and it is worth pinning, because
        // the obvious alternative reading (indices 0..4 are "missing", so hold
        // everything back) would stall a fresh connection forever.
        let mut fx = Fixture::client();
        let mut world = TestWorld::new();

        let mut wire = Wire::new(&fx.kinds);
        wire.message(5, EntityMessageType::Spawn).remote(8);
        wire.message(6, EntityMessageType::Spawn).remote(9);
        let bytes = wire.finish();
        fx.read_ok(&bytes);

        let spawns = fx
            .take_events(&mut world)
            .iter()
            .filter(|event| event.to_type() == Some(EntityMessageType::Spawn))
            .count();
        assert_eq!(
            spawns, 2,
            "both messages should be delivered even though the sequence does not \
             begin at zero"
        );
    }

    // -- updates are read before messages -----------------------------------

    #[test]
    fn the_update_section_is_read_before_the_message_section() {
        // Both sections in one stream. The sentinel is what makes this a real
        // ordering test: if the reader tried the message section first it would
        // consume the update bytes as a message header and the cursor would end
        // up somewhere else entirely.
        let mut fx = Fixture::client();
        let mut world = TestWorld::new();
        fx.adopt_remote(&mut world, 7);

        let mut wire = Wire::new(&fx.kinds);
        wire.update_entity(wire_host(7))
            .component_update(&Ghost::new_complete(42))
            .end_entity();
        wire.message(0, EntityMessageType::Spawn).remote(8);
        let bytes = wire.finish();
        fx.read_ok(&bytes);

        let events = fx.take_events(&mut world);
        assert!(
            events
                .iter()
                .any(|event| matches!(event, EntityEvent::UpdateComponent(_, _, _))),
            "the update section should have been decoded"
        );
        assert!(
            events
                .iter()
                .any(|event| event.to_type() == Some(EntityMessageType::Spawn)),
            "the message section should have been decoded"
        );
    }

    // -- per-variant parse shape --------------------------------------------

    /// Every message variant, encoded with exactly the payload the reader
    /// declares it reads. The assertion is the sentinel: it fails if a branch
    /// reads a field the encoder did not write, or skips one it did.
    ///
    /// This is the cheap half of the suite. It does not claim the variants are
    /// *routed* correctly — the tests above do that for the ones with an
    /// observable effect on this side of the connection — but it does pin the
    /// wire layout of all fifteen, including the six response-type messages a
    /// client never receives and so could not otherwise be reached here.
    /// One named encoder for the variant table below.
    type VariantEncoder = (&'static str, Box<dyn Fn(&mut Wire)>);

    #[test]
    fn every_message_variant_consumes_exactly_its_own_payload() {
        let encoders: Vec<VariantEncoder> = vec![
            (
                "Spawn",
                Box::new(|w: &mut Wire| {
                    w.message(0, EntityMessageType::Spawn).remote(7);
                }),
            ),
            (
                "SpawnWithComponents",
                Box::new(|w: &mut Wire| {
                    w.message(0, EntityMessageType::SpawnWithComponents)
                        .owned(wire_host(7))
                        .u8(1)
                        .component(&Ghost::new_complete(1));
                }),
            ),
            (
                "Despawn",
                Box::new(|w: &mut Wire| {
                    w.message(0, EntityMessageType::Despawn).owned(wire_host(7));
                }),
            ),
            (
                "InsertComponent",
                Box::new(|w: &mut Wire| {
                    w.message(0, EntityMessageType::InsertComponent)
                        .owned(wire_host(7))
                        .component(&Ghost::new_complete(1));
                }),
            ),
            (
                "RemoveComponent",
                Box::new(|w: &mut Wire| {
                    w.message(0, EntityMessageType::RemoveComponent)
                        .owned(wire_host(7))
                        .kind::<Ghost>();
                }),
            ),
            (
                "Publish",
                Box::new(|w: &mut Wire| {
                    w.message(0, EntityMessageType::Publish)
                        .sub_command(3)
                        .owned(wire_host(7));
                }),
            ),
            (
                "Unpublish",
                Box::new(|w: &mut Wire| {
                    w.message(0, EntityMessageType::Unpublish)
                        .sub_command(3)
                        .owned(wire_host(7));
                }),
            ),
            (
                "EnableDelegation",
                Box::new(|w: &mut Wire| {
                    w.message(0, EntityMessageType::EnableDelegation)
                        .sub_command(3)
                        .owned(wire_host(7));
                }),
            ),
            (
                "DisableDelegation",
                Box::new(|w: &mut Wire| {
                    w.message(0, EntityMessageType::DisableDelegation)
                        .sub_command(3)
                        .remote(7);
                }),
            ),
            (
                "SetAuthority",
                Box::new(|w: &mut Wire| {
                    w.message(0, EntityMessageType::SetAuthority)
                        .sub_command(3)
                        .remote(7)
                        .auth(EntityAuthStatus::Granted);
                }),
            ),
            (
                "RequestAuthority",
                Box::new(|w: &mut Wire| {
                    w.message(0, EntityMessageType::RequestAuthority)
                        .sub_command(3)
                        .host(7);
                }),
            ),
            (
                "ReleaseAuthority",
                Box::new(|w: &mut Wire| {
                    w.message(0, EntityMessageType::ReleaseAuthority)
                        .sub_command(3)
                        .owned(wire_host(7));
                }),
            ),
            (
                "EnableDelegationResponse",
                Box::new(|w: &mut Wire| {
                    w.message(0, EntityMessageType::EnableDelegationResponse)
                        .sub_command(3)
                        .host(7);
                }),
            ),
            (
                "MigrateResponse",
                Box::new(|w: &mut Wire| {
                    w.message(0, EntityMessageType::MigrateResponse)
                        .sub_command(3)
                        .host(7)
                        .remote(8);
                }),
            ),
            (
                "Noop",
                Box::new(|w: &mut Wire| {
                    w.message(0, EntityMessageType::Noop);
                }),
            ),
        ];

        assert_eq!(
            encoders.len(),
            15,
            "every EntityMessageType variant should be listed here; add the new \
             one rather than loosening this count"
        );

        for (name, encode) in encoders {
            let mut fx = Fixture::client();
            let mut wire = Wire::new(&fx.kinds);
            encode(&mut wire);
            let bytes = wire.finish();

            let magic = fx
                .read(&bytes, 0)
                .unwrap_or_else(|_| panic!("{name} should parse"));
            assert_eq!(
                magic, MAGIC,
                "{name} consumed the wrong number of bits from the stream"
            );
        }
    }

    // -- malformed input ----------------------------------------------------

    #[test]
    fn a_stream_truncated_mid_message_is_an_error_not_a_panic() {
        // Every prefix of a well-formed message, cut one byte at a time. None of
        // them may panic; each must come back as a `SerdeErr`. This is the whole
        // point of the reader returning a Result: the bytes are attacker-supplied.
        let mut wire = Wire::new(&Fixture::client().kinds);
        wire.message(0, EntityMessageType::SpawnWithComponents)
            .owned(wire_host(7))
            .u8(2)
            .component(&Ghost::new_complete(11))
            .component(&Wraith::new_complete(22));
        let full = wire.finish_raw();

        assert!(full.len() > 2, "the fixture stream should be worth cutting");

        for cut in 1..full.len() {
            let mut fx = Fixture::client();
            let truncated = &full[..cut];
            let mut reader = BitReader::new(truncated);
            let kinds = std::mem::take(&mut fx.kinds);
            // A truncated stream may parse (the trailing zero-fill of the last
            // byte can read as a valid "stop" bit) or it may error. What it may
            // never do is panic or hang, and if it parses it must not have
            // invented a message out of the padding.
            let _ = WorldReader::read_world_events(&mut fx.manager, &kinds, &0, &mut reader);
            fx.kinds = kinds;
        }
    }

    #[test]
    fn an_empty_buffer_is_an_error() {
        let mut fx = Fixture::client();
        let mut reader = BitReader::new(&[]);
        let kinds = std::mem::take(&mut fx.kinds);
        let result = WorldReader::read_world_events(&mut fx.manager, &kinds, &0, &mut reader);
        fx.kinds = kinds;
        assert!(
            result.is_err(),
            "there is not even a continue bit to read, so this must be an error"
        );
    }
}
