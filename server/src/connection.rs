use std::{
    hash::Hash,
    net::SocketAddr,
    sync::{Arc, RwLock},
};

use naia_shared::{
    sequence_greater_than,
    serde::{BitReader, BitWriter},
    BaseConnection, ChannelConfig, ChannelIndex, ConnectionConfig, EntityConverter, Instant,
    PacketType, PingManager, ProtocolIo, Protocolize, StandardHeader, Tick, TickBuffer,
    WorldRefType,
};

use super::{
    entity_manager::EntityManager, global_diff_handler::GlobalDiffHandler, io::Io,
    tick_manager::TickManager, user::UserKey, world_record::WorldRecord,
};

pub struct Connection<P: Protocolize, E: Copy + Eq + Hash, C: ChannelIndex> {
    pub user_key: UserKey,
    pub base: BaseConnection<P, C>,
    pub entity_manager: EntityManager<P, E, C>,
    pub tick_buffer: TickBuffer<P, C>,
    pub last_received_tick: Tick,
    pub ping_manager: PingManager,
}

impl<P: Protocolize, E: Copy + Eq + Hash, C: ChannelIndex> Connection<P, E, C> {
    pub fn new(
        connection_config: &ConnectionConfig,
        channel_config: &ChannelConfig<C>,
        user_address: SocketAddr,
        user_key: &UserKey,
        diff_handler: &Arc<RwLock<GlobalDiffHandler<E, P::Kind>>>,
    ) -> Self {
        Connection {
            user_key: *user_key,
            base: BaseConnection::new(user_address, connection_config, channel_config),
            entity_manager: EntityManager::new(user_address, diff_handler),
            tick_buffer: TickBuffer::new(channel_config),
            ping_manager: PingManager::new(&connection_config.ping),
            last_received_tick: 0,
        }
    }

    // Incoming Data

    pub fn process_incoming_header(
        &mut self,
        world_record: &WorldRecord<E, P::Kind>,
        header: &StandardHeader,
    ) {
        self.base
            .process_incoming_header(header, &mut Some(&mut self.entity_manager));
        self.entity_manager.process_delivered_packets(world_record);
    }

    pub fn recv_client_tick(&mut self, client_tick: Tick) {
        if sequence_greater_than(client_tick, self.last_received_tick) {
            self.last_received_tick = client_tick;
        }
    }

    pub fn process_incoming_data(
        &mut self,
        server_and_client_tick_opt: Option<(Tick, Tick)>,
        bit_reader: &mut BitReader,
        world_record: &WorldRecord<E, P::Kind>,
    ) {
        if let Some((server_tick, client_tick)) = server_and_client_tick_opt {
            // Read Tick Buffered Messages
            let mut converter = EntityConverter::new(world_record, &self.entity_manager);
            self.tick_buffer
                .read_messages(&server_tick, &client_tick, bit_reader, &mut converter);
        }

        // Read Messages
        {
            let converter = EntityConverter::new(world_record, &self.entity_manager);
            let channel_writer = ProtocolIo::new(&converter);
            self.base
                .message_manager
                .read_messages(&channel_writer, bit_reader);
        }
    }

    // Outgoing data
    pub fn send_outgoing_packets<W: WorldRefType<P, E>>(
        &mut self,
        now: &Instant,
        io: &mut Io,
        world: &W,
        world_record: &WorldRecord<E, P::Kind>,
        tick_manager_opt: &Option<TickManager>,
        rtt_millis: &f32,
    ) {
        self.collect_outgoing_messages(rtt_millis);

        let mut any_sent = false;
        loop {
            if self.send_outgoing_packet(now, io, world, world_record, tick_manager_opt) {
                any_sent = true;
            } else {
                break;
            }
        }
        if any_sent {
            self.base.mark_sent();
        }
    }

    fn collect_outgoing_messages(&mut self, rtt_millis: &f32) {
        self.entity_manager
            .collect_outgoing_messages(rtt_millis, &mut self.base.message_manager);
        self.base
            .message_manager
            .collect_outgoing_messages(rtt_millis);
    }

    fn send_outgoing_packet<W: WorldRefType<P, E>>(
        &mut self,
        now: &Instant,
        io: &mut Io,
        world: &W,
        world_record: &WorldRecord<E, P::Kind>,
        tick_manager_opt: &Option<TickManager>,
    ) -> bool {
        if self.base.message_manager.has_outgoing_messages()
            || self.entity_manager.has_outgoing_messages()
        {
            let next_packet_index = self.base.next_packet_index();

            let mut bit_writer = BitWriter::new();

            // write header
            self.base
                .write_outgoing_header(PacketType::Data, &mut bit_writer);

            // write server tick
            if let Some(tick_manager) = tick_manager_opt {
                tick_manager.write_server_tick(&mut bit_writer);
            }

            // write messages
            {
                let converter = EntityConverter::new(world_record, &self.entity_manager);
                let channel_writer = ProtocolIo::new(&converter);
                self.base.message_manager.write_messages(
                    &channel_writer,
                    &mut bit_writer,
                    next_packet_index,
                );
            }

            // write entity actions
            self.entity_manager.write_all(
                now,
                &mut bit_writer,
                &next_packet_index,
                world,
                world_record,
            );

            // send packet
            io.send_writer(&self.base.address, &mut bit_writer);

            return true;
        }

        return false;
    }
}
