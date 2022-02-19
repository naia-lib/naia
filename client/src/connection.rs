use std::{hash::Hash, net::SocketAddr};

use naia_client_socket::Packet;

use naia_shared::{
    BaseConnection, ConnectionConfig, ManagerType, Manifest, PacketReader, PacketType, Protocolize,
    ReplicateSafe, StandardHeader, WorldMutType,
};
use crate::types::PacketIndex;

use super::{
    entity_action::EntityAction,
    entity_manager::EntityManager,
    ping_manager::PingManager,
    tick_manager::TickManager,
    tick_queue::TickQueue,
    types::Tick,
};

pub struct Connection<P: Protocolize, E: Copy + Eq + Hash> {
    base_connection: BaseConnection<P>,
    entity_manager: EntityManager<P, E>,
    ping_manager: PingManager,
    jitter_buffer: TickQueue<Box<[u8]>>,
}

impl<P: Protocolize, E: Copy + Eq + Hash> Connection<P, E> {
    pub fn new(address: SocketAddr, connection_config: &ConnectionConfig) -> Self {
        return Connection {
            base_connection: BaseConnection::new(address, connection_config),
            entity_manager: EntityManager::new(),
            ping_manager: PingManager::new(
                connection_config.ping_interval,
                connection_config.rtt_initial_estimate,
                connection_config.jitter_initial_estimate,
                connection_config.rtt_smoothing_factor,
            ),
            jitter_buffer: TickQueue::new(),
        };
    }

    // Incoming Data
    pub fn process_incoming_data<W: WorldMutType<P, E>>(
        &mut self,
        world: &mut W,
        manifest: &Manifest<P>,
        server_tick: u16,
        data: &[u8],
    ) {
        let mut reader = PacketReader::new(data);
        while reader.has_more() {
            let manager_type: ManagerType = reader.read_u8().into();
            match manager_type {
                ManagerType::Message => {
                    self.base_connection
                        .process_message_data(&mut reader, manifest);
                }
                ManagerType::Entity => {
                    self.entity_manager
                        .process_data(world, manifest, server_tick, &mut reader);
                }
                _ => {}
            }
        }
    }

    // Outgoing Data
    pub fn outgoing_packet(
        &mut self,
        client_tick: u16,
    ) -> Option<Box<[u8]>> {
        if self.base_connection.has_outgoing_messages() || self.entity_manager.has_outgoing_messages() {

            let next_packet_index: PacketIndex = self.next_packet_index();

            // Write Entity Messages
            self.entity_manager.write_messages(
                self.base_connection.writer_bytes_number() + self.entity_manager.writer_bytes_number(),
                next_packet_index,
            );

            // Write Messages
            self.base_connection.write_messages(
                self.base_connection.writer_bytes_number() + self.entity_manager.writer_bytes_number(),
                next_packet_index,
            );

            // Add header
            if self.base_connection.writer_has_bytes() || self.entity_manager.writer_has_bytes() {
                // Get bytes from writer
                let mut out_vec = Vec::<u8>::new();
                self.base_connection.writer_bytes(&mut out_vec);
                self.entity_manager.writer_bytes(&mut out_vec);

                // Add header to it
                let payload =
                    self.process_outgoing_header(client_tick, PacketType::Data, &out_vec.into_boxed_slice());
                return Some(payload);
            } else {
                panic!("Pending outgoing messages but no bytes were written... Likely trying to transmit a Component/Message larger than 576 bytes!");
            }
        }

        return None;
    }

    pub fn buffer_data_packet(&mut self, incoming_tick: u16, incoming_payload: &Box<[u8]>) {
        self.jitter_buffer
            .add_item(incoming_tick, incoming_payload.clone());
    }

    // Entity Manager

    pub fn incoming_entity_action(&mut self) -> Option<EntityAction<P, E>> {
        return self.entity_manager.pop_incoming_message();
    }

    pub fn send_entity_message<R: ReplicateSafe<P>>(
        &mut self,
        entity: &E,
        message: &R,
        client_tick: u16,
    ) {
        return self
            .entity_manager
            .send_entity_message(entity, message, client_tick);
    }

    pub fn on_tick(&mut self, server_receivable_tick: Tick) {
        self.entity_manager.entity_message_sender.on_tick(server_receivable_tick);
    }

    pub fn process_buffered_packets<W: WorldMutType<P, E>>(
        &mut self,
        world: &mut W,
        manifest: &Manifest<P>,
        receiving_tick: u16,
    ) {
        while let Some((server_tick, data_packet)) = self.jitter_buffer.pop_item(receiving_tick) {

            self.process_incoming_data(world, manifest, server_tick, &data_packet);
        }
    }

    // Pass-through methods to underlying Connection

    pub fn mark_sent(&mut self) {
        return self.base_connection.mark_sent();
    }

    pub fn should_send_heartbeat(&self) -> bool {
        return self.base_connection.should_send_heartbeat();
    }

    pub fn mark_heard(&mut self) {
        return self.base_connection.mark_heard();
    }

    pub fn should_drop(&self) -> bool {
        return self.base_connection.should_drop();
    }

    pub fn process_incoming_header(
        &mut self,
        header: &StandardHeader,
        tick_manager_opt: Option<&mut TickManager>,
    ) {
        if let Some(tick_manager) = tick_manager_opt {
            tick_manager.record_server_tick(
                header.host_tick(),
                self.ping_manager.rtt(),
                self.ping_manager.jitter(),
            );
        }

        self.base_connection
            .process_incoming_header(header, &mut Some(&mut self.entity_manager.entity_message_sender));
    }

    pub fn process_outgoing_header(
        &mut self,
        client_tick: u16,
        packet_type: PacketType,
        payload: &[u8],
    ) -> Box<[u8]> {
        return self
            .base_connection
            .process_outgoing_header(client_tick, packet_type, payload);
    }

    pub fn next_packet_index(&self) -> PacketIndex {
        return self.base_connection.next_packet_index();
    }

    pub fn send_message<R: ReplicateSafe<P>>(&mut self, message: &R, guaranteed_delivery: bool) {
        return self
            .base_connection
            .send_message(message, guaranteed_delivery);
    }

    pub fn incoming_message(&mut self) -> Option<P> {
        return self.base_connection.incoming_message();
    }

    // Ping related
    pub fn should_send_ping(&self) -> bool {
        return self.ping_manager.should_send_ping();
    }

    pub fn ping_packet(&mut self) -> Packet {
        self.ping_manager.ping_packet()
    }

    pub fn process_pong(&mut self, pong_payload: &[u8]) {
        self.ping_manager.process_pong(pong_payload);
    }

    pub fn rtt(&self) -> f32 {
        return self.ping_manager.rtt();
    }

    pub fn jitter(&self) -> f32 {
        return self.ping_manager.jitter();
    }
}
