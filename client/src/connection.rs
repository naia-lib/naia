use std::{collections::VecDeque, hash::Hash, net::SocketAddr};

use naia_shared::{serde::{BitReader, BitWriter, OwnedBitReader}, BaseConnection, ChannelConfig, ChannelIndex, ConnectionConfig, Manifest, PacketType, PingConfig, Protocolize, StandardHeader, Tick, WorldMutType};

use super::{
    entity_manager::EntityManager, error::NaiaClientError, event::Event, ping_manager::PingManager,
    tick_queue::TickQueue, io::Io, tick_buffer::TickBuffer, tick_manager::TickManager,
};

pub struct Connection<P: Protocolize, E: Copy + Eq + Hash, C: ChannelIndex> {
    pub base: BaseConnection<P, C>,
    pub entity_manager: EntityManager<P, E>,
    pub ping_manager: Option<PingManager>,
    pub tick_buffer_message_sender: TickBuffer<P, C>,
    jitter_buffer: TickQueue<OwnedBitReader>,
}

impl<P: Protocolize, E: Copy + Eq + Hash, C: ChannelIndex> Connection<P, E, C> {
    pub fn new(
        address: SocketAddr,
        connection_config: &ConnectionConfig,
        channel_config: &ChannelConfig<C>,
        ping_config: &Option<PingConfig>,
    ) -> Self {
        let ping_manager: Option<PingManager> = ping_config.as_ref().map(|config| {
            PingManager::new(
                config.ping_interval,
                config.rtt_initial_estimate,
                config.jitter_initial_estimate,
                config.rtt_smoothing_factor,
            )
        });

        return Connection {
            base: BaseConnection::new(address, connection_config, channel_config),
            entity_manager: EntityManager::new(),
            ping_manager,
            tick_buffer_message_sender: TickBuffer::new(channel_config),
            jitter_buffer: TickQueue::new(),
        };
    }

    // Incoming data

    pub fn process_incoming_header(&mut self, header: &StandardHeader) {
        self.base
            .process_incoming_header(header, &mut Some(&mut self.tick_buffer_message_sender));
    }

    pub fn buffer_data_packet(&mut self, incoming_tick: Tick, reader: &mut BitReader) {
        self.jitter_buffer
            .add_item(incoming_tick, reader.to_owned());
    }

    pub fn process_buffered_packets<W: WorldMutType<P, E>>(
        &mut self,
        world: &mut W,
        manifest: &Manifest<P>,
        receiving_tick: Tick,
        incoming_events: &mut VecDeque<Result<Event<P, E, C>, NaiaClientError>>,
    ) {
        while let Some((server_tick, owned_reader)) = self.jitter_buffer.pop_item(receiving_tick) {
            let mut reader = owned_reader.borrow();

            // Read Messages
            self.base
                .message_manager
                .read_messages(&mut reader, manifest, &self.entity_manager);

            // Read Entity Actions
            self.entity_manager.read_actions(
                world,
                manifest,
                server_tick,
                &mut reader,
                incoming_events,
            );
        }
    }

    // Outgoing data

    pub fn send_outgoing_packets(&mut self, io: &mut Io, tick_manager_opt: &Option<TickManager>) {

        self.generate_resend_messages(tick_manager_opt);

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

    fn generate_resend_messages(&mut self, tick_manager_opt: &Option<TickManager>) {
        if let Some(ping_manager) = &self.ping_manager {
            self.base.message_manager.generate_resend_messages(&ping_manager.rtt);
        }
        if let Some(tick_manager) = tick_manager_opt {
            self.tick_buffer_message_sender.generate_resend_messages(&tick_manager.server_receivable_tick());
        }
    }

    // Sends packet and returns whether or not a packet was sent
    fn send_outgoing_packet(
        &mut self,
        io: &mut Io,
        tick_manager_opt: &Option<TickManager>,
    ) -> bool {
        if self.base.message_manager.has_outgoing_messages()
            || self.tick_buffer_message_sender.has_outgoing_messages()
        {
            let next_packet_index = self.base.next_packet_index();

            let mut writer = BitWriter::new();

            // write header
            self.base
                .write_outgoing_header(PacketType::Data, &mut writer);

            if let Some(tick_manager) = tick_manager_opt {
                // write tick
                tick_manager.write_client_tick(&mut writer);

                // write tick buffered messages
                self.tick_buffer_message_sender.write_messages(
                    &self.entity_manager,
                    &mut writer,
                    next_packet_index,
                );
            }

            // write messages
            self.base.message_manager.write_messages(
                &mut writer,
                next_packet_index,
                &self.entity_manager,
            );

            // send packet
            io.send_writer(&mut writer);

            return true;
        }

        return false;
    }
}
