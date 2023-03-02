use std::{hash::Hash, net::SocketAddr};

use log::warn;

use naia_shared::{
    BaseConnection, BitReader, BitWriter, ChannelKinds, ConnectionConfig, EntityActionEvent,
    HostGlobalWorldManager, HostLocalWorldManager, HostType, Instant, OwnedBitReader, PacketType,
    Protocol, RemoteWorldManager, Serde, SerdeErr, StandardHeader, Tick, WorldMutType,
};

use crate::{
    connection::{
        tick_buffer_sender::TickBufferSender, tick_queue::TickQueue, time_manager::TimeManager,
    },
    events::Events,
};

use super::io::Io;

pub struct Connection<E: Copy + Eq + Hash + Send + Sync> {
    pub base: BaseConnection,
    pub host_world_manager: HostLocalWorldManager<E>,
    pub remote_world_manager: RemoteWorldManager<E>,
    pub time_manager: TimeManager,
    pub tick_buffer: TickBufferSender,
    /// Small buffer when receiving updates (entity actions, entity updates) from the server
    /// to make sure we receive them in order
    jitter_buffer: TickQueue<OwnedBitReader>,
}

impl<E: Copy + Eq + Hash + Send + Sync> Connection<E> {
    pub fn new(
        address: SocketAddr,
        connection_config: &ConnectionConfig,
        channel_kinds: &ChannelKinds,
        time_manager: TimeManager,
        host_global_world_manager: &HostGlobalWorldManager<E>,
    ) -> Self {
        let tick_buffer = TickBufferSender::new(channel_kinds);

        let mut connection = Connection {
            base: BaseConnection::new(
                address.clone(),
                HostType::Client,
                connection_config,
                channel_kinds,
            ),
            host_world_manager: HostLocalWorldManager::new(
                address,
                host_global_world_manager.diff_handler(),
            ),
            remote_world_manager: RemoteWorldManager::new(),
            time_manager,
            tick_buffer,
            jitter_buffer: TickQueue::new(),
        };

        let existing_entities = host_global_world_manager.entities();
        for entity in existing_entities {
            let component_kinds = host_global_world_manager.component_kinds(&entity).unwrap();
            connection
                .host_world_manager
                .init_entity(&entity, component_kinds);
        }

        connection
    }

    // Incoming data

    pub fn process_incoming_header(&mut self, header: &StandardHeader) {
        self.base
            .process_incoming_header(header, &mut Some(&mut self.tick_buffer));
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
        incoming_events: &mut Events<E>,
    ) {
        let receiving_tick = self.time_manager.client_receiving_tick;

        while let Some((server_tick, owned_reader)) = self.jitter_buffer.pop_item(receiving_tick) {
            let mut reader = owned_reader.borrow();

            // read messages
            {
                let messages_result = self.base.message_manager.read_messages(
                    protocol,
                    &self.remote_world_manager,
                    &mut reader,
                );
                if messages_result.is_err() {
                    // TODO: Except for cosmic radiation .. Server should never send a malformed packet .. handle this
                    warn!("Error reading incoming messages from packet!");
                    continue;
                }
            }

            // read entity updates
            {
                let Ok(events) = self.remote_world_manager.read_updates(&protocol.component_kinds, world, server_tick, &mut reader) else {
                    // TODO: Except for cosmic radiation .. Server should never send a malformed packet .. handle this
                    warn!("Error reading incoming entity updates from packet!");
                    continue;
                };
                for (tick, entity, component_kind) in events {
                    incoming_events.push_update(tick, entity, component_kind);
                }
            }

            // read entity actions
            {
                let Ok(events) = self.remote_world_manager.read_actions(&protocol.component_kinds, world, &mut reader) else {
                    // TODO: Except for cosmic radiation .. Server should never send a malformed packet .. handle this
                    warn!("Error reading incoming entity actions from packet!");
                    continue;
                };
                for event in events {
                    match event {
                        EntityActionEvent::SpawnEntity(entity) => {
                            incoming_events.push_spawn(entity);
                        }
                        EntityActionEvent::DespawnEntity(entity) => {
                            incoming_events.push_despawn(entity);
                        }
                        EntityActionEvent::InsertComponent(entity, component_kind) => {
                            incoming_events.push_insert(entity, component_kind);
                        }
                        EntityActionEvent::RemoveComponent(entity, component) => {
                            incoming_events.push_remove(entity, component);
                        }
                    }
                }
            }
        }
    }

    pub fn receive_messages(&mut self, incoming_events: &mut Events<E>) {
        let messages = self.base.message_manager.receive_messages();
        for (channel_kind, message) in messages {
            incoming_events.push_message(&channel_kind, message);
        }
    }

    // Outgoing data

    /// Collect and send any outgoing packets from client to server
    ///
    /// Outgoing packets are either:
    /// * messages
    /// * acks from reliable channels
    /// * acks from the `EntityActionReceiver` for all [`EntityAction`]s
    pub fn send_outgoing_packets(&mut self, protocol: &Protocol, io: &mut Io) {
        self.collect_outgoing_messages();

        let mut any_sent = false;
        loop {
            if self.send_outgoing_packet(protocol, io) {
                any_sent = true;
            } else {
                break;
            }
        }
        if any_sent {
            self.base.mark_sent();
        }
    }

    fn collect_outgoing_messages(&mut self) {
        let now = Instant::now();

        self.base
            .message_manager
            .collect_outgoing_messages(&now, &self.time_manager.rtt());

        self.tick_buffer.collect_outgoing_messages(
            &self.time_manager.client_sending_tick,
            &self.time_manager.server_receivable_tick,
        );
    }

    // Sends packet and returns whether or not a packet was sent
    fn send_outgoing_packet(&mut self, protocol: &Protocol, io: &mut Io) -> bool {
        let tick_buffer_has_outgoing_messages = self.tick_buffer.has_outgoing_messages();

        if self.base.message_manager.has_outgoing_messages() || tick_buffer_has_outgoing_messages {
            let next_packet_index = self.base.next_packet_index();

            let mut writer = BitWriter::new();

            // Reserve bits we know will be required to finish the message:
            // 1. Tick buffer finish bit
            // 2. Messages finish bit
            writer.reserve_bits(2);

            // write header
            self.base
                .write_outgoing_header(PacketType::Data, &mut writer);

            let mut has_written = false;

            // write tick
            let client_tick: Tick = self.time_manager.client_sending_tick;
            client_tick.ser(&mut writer);

            // write tick buffered messages
            self.tick_buffer.write_messages(
                &protocol,
                &self.remote_world_manager,
                &mut writer,
                next_packet_index,
                &client_tick,
                &mut has_written,
            );

            // finish tick buffered messages
            false.ser(&mut writer);
            writer.release_bits(1);

            // write messages
            {
                self.base.message_manager.write_messages(
                    protocol,
                    &self.remote_world_manager,
                    &mut writer,
                    next_packet_index,
                    &mut has_written,
                );

                // finish messages
                false.ser(&mut writer);
                writer.release_bits(1);
            }

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
