use std::{collections::VecDeque, hash::Hash, net::SocketAddr};

use naia_shared::{
    serde::{BitReader, BitWriter, OwnedBitReader},
    BaseConnection, ChannelConfig, ChannelIndex, ConnectionConfig, PacketType, PingManager,
    ProtocolIo, Protocolize, StandardHeader, Tick, TickBuffer, WorldMutType,
};

use super::{
    entity_manager::EntityManager, error::NaiaClientError, event::Event, io::Io,
    tick_manager::TickManager, tick_queue::TickQueue,
};

pub struct Connection<P: Protocolize, E: Copy + Eq + Hash, C: ChannelIndex> {
    pub base: BaseConnection<P, C>,
    pub entity_manager: EntityManager<P, E>,
    pub ping_manager: PingManager,
    pub tick_buffer: TickBuffer<P, C>,
    jitter_buffer: TickQueue<OwnedBitReader>,
}

impl<P: Protocolize, E: Copy + Eq + Hash, C: ChannelIndex> Connection<P, E, C> {
    pub fn new(
        address: SocketAddr,
        connection_config: &ConnectionConfig,
        channel_config: &ChannelConfig<C>,
    ) -> Self {
        return Connection {
            base: BaseConnection::new(address, connection_config, channel_config),
            entity_manager: EntityManager::new(),
            ping_manager: PingManager::new(&connection_config.ping),
            tick_buffer: TickBuffer::new(channel_config),
            jitter_buffer: TickQueue::new(),
        };
    }

    // Incoming data

    pub fn process_incoming_header(&mut self, header: &StandardHeader) {
        self.base
            .process_incoming_header(header, &mut Some(&mut self.tick_buffer));
    }

    pub fn buffer_data_packet(&mut self, incoming_tick: Tick, reader: &mut BitReader) {
        self.jitter_buffer
            .add_item(incoming_tick, reader.to_owned());
    }

    pub fn process_buffered_packets<W: WorldMutType<P, E>>(
        &mut self,
        world: &mut W,
        receiving_tick: Tick,
        incoming_events: &mut VecDeque<Result<Event<P, E, C>, NaiaClientError>>,
    ) {
        while let Some((server_tick, owned_reader)) = self.jitter_buffer.pop_item(receiving_tick) {
            let mut bit_reader = owned_reader.borrow();

            let channel_reader = ProtocolIo::new(&self.entity_manager);

            // Read Messages
            self.base
                .message_manager
                .read_messages(&channel_reader, &mut bit_reader);

            // Read Entity Actions
            self.entity_manager
                .read_actions(world, server_tick, &mut bit_reader, incoming_events);
        }
    }

    // Outgoing data

    pub fn send_outgoing_packets(&mut self, io: &mut Io, tick_manager_opt: &Option<TickManager>) {
        self.collect_outgoing_messages(tick_manager_opt);

        let mut any_sent = false;
        loop {
            if self.send_outgoing_packet(io, tick_manager_opt) {
                any_sent = true;
            } else {
                break;
            }
        }
        if any_sent {
            self.base.mark_sent();
        }
    }

    fn collect_outgoing_messages(&mut self, tick_manager_opt: &Option<TickManager>) {
        self.base
            .message_manager
            .collect_outgoing_messages(&self.ping_manager.rtt);
        if let Some(tick_manager) = tick_manager_opt {
            self.tick_buffer.collect_outgoing_messages(
                &tick_manager.client_sending_tick(),
                &tick_manager.server_receivable_tick(),
            );
        }
    }

    // Sends packet and returns whether or not a packet was sent
    fn send_outgoing_packet(
        &mut self,
        io: &mut Io,
        tick_manager_opt: &Option<TickManager>,
    ) -> bool {
        if self.base.message_manager.has_outgoing_messages()
            || self.tick_buffer.has_outgoing_messages()
        {
            let next_packet_index = self.base.next_packet_index();

            let mut bit_writer = BitWriter::new();

            // write header
            self.base
                .write_outgoing_header(PacketType::Data, &mut bit_writer);

            let channel_writer = ProtocolIo::new(&self.entity_manager);

            if let Some(tick_manager) = tick_manager_opt {
                // write tick
                let client_tick = tick_manager.write_client_tick(&mut bit_writer);

                // write tick buffered messages
                self.tick_buffer.write_messages(
                    &channel_writer,
                    &mut bit_writer,
                    next_packet_index,
                    &client_tick,
                );
            }

            // write messages
            self.base.message_manager.write_messages(
                &channel_writer,
                &mut bit_writer,
                next_packet_index,
            );

            // send packet
            io.send_writer(&mut bit_writer);

            return true;
        }

        return false;
    }
}
