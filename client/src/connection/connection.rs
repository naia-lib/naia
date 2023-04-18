use std::hash::Hash;

use log::warn;

use naia_shared::{
    BaseConnection, BitReader, BitWriter, ChannelKinds, ConnectionConfig, EntityConverter,
    EntityEvent, GlobalWorldManagerType, HostType, Instant, OwnedBitReader, PacketType, Protocol,
    Serde, SerdeErr, StandardHeader, Tick, WorldMutType, WorldRefType,
};

use crate::{
    connection::{
        io::Io, tick_buffer_sender::TickBufferSender, tick_queue::TickQueue,
        time_manager::TimeManager,
    },
    events::Events,
    world::global_world_manager::GlobalWorldManager,
};

pub struct Connection<E: Copy + Eq + Hash + Send + Sync> {
    pub base: BaseConnection<E>,
    pub time_manager: TimeManager,
    pub tick_buffer: TickBufferSender,
    /// Small buffer when receiving updates (entity actions, entity updates) from the server
    /// to make sure we receive them in order
    jitter_buffer: TickQueue<OwnedBitReader>,
}

impl<E: Copy + Eq + Hash + Send + Sync> Connection<E> {
    pub fn new(
        connection_config: &ConnectionConfig,
        channel_kinds: &ChannelKinds,
        time_manager: TimeManager,
        global_world_manager: &GlobalWorldManager<E>,
    ) -> Self {
        let tick_buffer = TickBufferSender::new(channel_kinds);

        let mut connection = Connection {
            base: BaseConnection::new(
                &None,
                HostType::Client,
                connection_config,
                channel_kinds,
                global_world_manager,
            ),
            time_manager,
            tick_buffer,
            jitter_buffer: TickQueue::new(),
        };

        let existing_entities = global_world_manager.entities();
        for entity in existing_entities {
            let component_kinds = global_world_manager.component_kinds(&entity).unwrap();
            connection.base.host_world_manager.init_entity(
                &mut connection.base.local_world_manager,
                &entity,
                component_kinds,
            );
        }

        connection
    }

    // Incoming data

    pub fn process_incoming_header(&mut self, header: &StandardHeader) {
        self.base
            .process_incoming_header(header, &mut [&mut self.tick_buffer]);
    }

    pub fn buffer_data_packet(
        &mut self,
        incoming_tick: &Tick,
        reader: &mut BitReader,
    ) -> Result<(), SerdeErr> {
        self.jitter_buffer
            .add_item(*incoming_tick, reader.to_owned());
        Ok(())
    }

    /// Read the packets (raw bits) from the jitter buffer that correspond to the
    /// `receiving_tick`
    ///
    /// * Receive (process) entity actions/entity updates and emit events for them
    /// * Read messages and store them into an internal buffer
    ///
    /// Note that currently, messages are also being stored in the jitter buffer and processed
    /// on the receiving tick, even though it's not needed is the channel is not tick buffered.
    pub fn process_buffered_packets<W: WorldMutType<E>>(
        &mut self,
        protocol: &Protocol,
        world: &mut W,
        global_world_manager: &mut GlobalWorldManager<E>,
        incoming_events: &mut Events<E>,
    ) -> Result<(), SerdeErr> {
        let receiving_tick = self.time_manager.client_receiving_tick;

        while let Some((server_tick, owned_reader)) = self.jitter_buffer.pop_item(receiving_tick) {
            let mut reader = owned_reader.borrow();

            // read messages
            {
                let entity_converter =
                    EntityConverter::new(global_world_manager, &self.base.local_world_manager);
                let messages = self.base.message_manager.read_messages(
                    protocol,
                    &entity_converter,
                    &mut reader,
                )?;
                for (channel_kind, messages) in messages {
                    for message in messages {
                        incoming_events.push_message(&channel_kind, message);
                    }
                }
            }

            // read world events
            let entity_events = self.base.remote_world_manager.read_world_events(
                global_world_manager,
                &mut self.base.local_world_manager,
                protocol,
                world,
                server_tick,
                &mut reader,
            )?;

            for event in &entity_events {
                match event {
                    EntityEvent::SpawnEntity(entity) => {
                        global_world_manager.remote_spawn_entity(entity);
                    }
                    EntityEvent::DespawnEntity(entity) => {
                        global_world_manager.remote_despawn_entity(entity);
                    }
                    _ => {}
                }
            }

            incoming_events.receive_entity_events(entity_events);
        }

        Ok(())
    }

    // Outgoing data

    /// Collect and send any outgoing packets from client to server
    pub fn send_outgoing_packets<W: WorldRefType<E>>(
        &mut self,
        protocol: &Protocol,
        now: &Instant,
        io: &mut Io,
        world: &W,
        global_world_manager: &GlobalWorldManager<E>,
    ) {
        let rtt_millis = self.time_manager.rtt();
        self.base.collect_outgoing_messages(
            now,
            &rtt_millis,
        );

        self.tick_buffer.collect_outgoing_messages(
            &self.time_manager.client_sending_tick,
            &self.time_manager.server_receivable_tick,
        );

        let mut any_sent = false;
        loop {
            if self.send_outgoing_packet(protocol, now, io, world, global_world_manager) {
                any_sent = true;
            } else {
                break;
            }
        }
        if any_sent {
            self.base.mark_sent();
        }
    }

    // Sends packet and returns whether or not a packet was sent
    fn send_outgoing_packet<W: WorldRefType<E>>(
        &mut self,
        protocol: &Protocol,
        now: &Instant,
        io: &mut Io,
        world: &W,
        global_world_manager: &GlobalWorldManager<E>,
    ) -> bool {
        if self.base.has_outgoing_messages() || self.tick_buffer.has_outgoing_messages() {
            let next_packet_index = self.base.next_packet_index();

            let mut writer = BitWriter::new();

            // Reserve bits we know will be required to finish the message:
            // 1. Tick buffer finish bit
            // 2. Messages finish bit
            // 3. Updates finish bit
            // 4. Actions finish bit
            writer.reserve_bits(4);

            // write header
            self.base
                .write_outgoing_header(PacketType::Data, &mut writer);

            // write client tick
            let client_tick: Tick = self.time_manager.client_sending_tick;
            client_tick.ser(&mut writer);

            let mut has_written = false;

            // write tick buffered messages
            {
                let converter =
                    EntityConverter::new(global_world_manager, &self.base.local_world_manager);
                self.tick_buffer.write_messages(
                    &protocol,
                    &converter,
                    &mut writer,
                    next_packet_index,
                    &client_tick,
                    &mut has_written,
                );

                // finish tick buffered messages
                false.ser(&mut writer);
                writer.release_bits(1);
            }

            self.base.write_outgoing_packet(
                protocol,
                now,
                &mut writer,
                next_packet_index,
                world,
                global_world_manager,
                &mut has_written,
                protocol.client_authoritative_entities,
            );

            // send packet
            if io.send_packet(writer.to_packet()).is_err() {
                // TODO: pass this on and handle above
                warn!("Client Error: Cannot send data packet to Server");
            }

            return true;
        }

        false
    }
}
