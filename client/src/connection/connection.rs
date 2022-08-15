use std::{collections::VecDeque, hash::Hash, net::SocketAddr, time::Duration};

use naia_shared::{
    serde::{BitReader, BitWriter, OwnedBitReader},
    BaseConnection, ChannelConfig, ChannelIndex, ConnectionConfig, HostType, Instant, PacketType,
    PingManager, ProtocolIo, Protocolize, StandardHeader, Tick, WorldMutType,
};

use crate::{
    error::NaiaClientError,
    event::Event,
    protocol::entity_manager::EntityManager,
    tick::{
        tick_buffer_sender::TickBufferSender, tick_manager::TickManager, tick_queue::TickQueue,
    },
};

use super::io::Io;

pub struct Connection<P: Protocolize, E: Copy + Eq + Hash, C: ChannelIndex> {
    pub base: BaseConnection<P, C>,
    pub entity_manager: EntityManager<P, E>,
    pub ping_manager: PingManager,
    pub tick_buffer: Option<TickBufferSender<P, C>>,
    jitter_buffer: TickQueue<OwnedBitReader>,
}

impl<P: Protocolize, E: Copy + Eq + Hash, C: ChannelIndex> Connection<P, E, C> {
    pub fn new(
        address: SocketAddr,
        connection_config: &ConnectionConfig,
        channel_config: &ChannelConfig<C>,
        tick_duration: &Option<Duration>,
    ) -> Self {
        let tick_buffer = tick_duration
            .as_ref()
            .map(|duration| TickBufferSender::new(channel_config, duration));

        Connection {
            base: BaseConnection::new(address, HostType::Client, connection_config, channel_config),
            entity_manager: EntityManager::default(),
            ping_manager: PingManager::new(&connection_config.ping),
            tick_buffer,
            jitter_buffer: TickQueue::new(),
        }
    }

    // Incoming data

    pub fn process_incoming_header(&mut self, header: &StandardHeader) {
        match &mut self.tick_buffer {
            Some(tick_buffer) => self
                .base
                .process_incoming_header(header, &mut Some(tick_buffer)),
            None => self.base.process_incoming_header(header, &mut None),
        }
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
                .read_all(world, server_tick, &mut bit_reader, incoming_events);
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
        let now = Instant::now();

        self.base
            .message_manager
            .collect_outgoing_messages(&now, &self.ping_manager.rtt);

        if let Some(tick_manager) = tick_manager_opt {
            self.tick_buffer
                .as_mut()
                .unwrap()
                .collect_outgoing_messages(
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
        let tick_buffer_has_outgoing_messages = match &self.tick_buffer {
            Some(tick_buffer) => tick_buffer.has_outgoing_messages(),
            None => false,
        };

        if self.base.message_manager.has_outgoing_messages() || tick_buffer_has_outgoing_messages {
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

                self.tick_buffer.as_mut().unwrap().write_messages(
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

        false
    }
}
