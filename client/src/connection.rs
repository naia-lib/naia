use std::{collections::VecDeque, hash::Hash, net::SocketAddr};

use naia_shared::{
    BaseConnection, ConnectionConfig, ManagerType, Manifest, PacketType,
    PingConfig, Protocolize, StandardHeader, Tick, WorldMutType, serde::{Serde, BitReader, BitWriter, FrozenBitReader}
};

use super::{
    entity_manager::EntityManager, error::NaiaClientError, event::Event, ping_manager::PingManager,
    tick_manager::TickManager, tick_queue::TickQueue,
};

pub struct Connection<P: Protocolize, E: Copy + Eq + Hash> {
    pub base: BaseConnection<P>,
    pub entity_manager: EntityManager<P, E>,
    pub ping_manager: Option<PingManager>,
    jitter_buffer: TickQueue<FrozenBitReader>,
}

impl<P: Protocolize, E: Copy + Eq + Hash> Connection<P, E> {
    pub fn new(
        address: SocketAddr,
        connection_config: &ConnectionConfig,
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
            base: BaseConnection::new(address, connection_config),
            entity_manager: EntityManager::new(),
            ping_manager,
            jitter_buffer: TickQueue::new(),
        };
    }

    // Incoming data

    pub fn process_incoming_header(
        &mut self,
        header: &StandardHeader,
        tick_manager_opt: Option<&mut TickManager>,
    ) {
        if let Some(tick_manager) = tick_manager_opt {
            if let Some(ping_manager) = &self.ping_manager {
                tick_manager.record_server_tick(
                    header.host_tick(),
                    ping_manager.rtt,
                    ping_manager.jitter,
                );
            }
        }

        self.base
            .process_incoming_header(header, &mut Some(&mut self.entity_manager.message_sender));
    }

    pub fn buffer_data_packet(&mut self, incoming_tick: u16, reader: &mut BitReader) {
        self.jitter_buffer
            .add_item(incoming_tick, reader.freeze());
    }

    pub fn process_buffered_packets<W: WorldMutType<P, E>>(
        &mut self,
        world: &mut W,
        manifest: &Manifest<P>,
        receiving_tick: Tick,
        incoming_events: &mut VecDeque<Result<Event<P, E>, NaiaClientError>>,
    ) {
        while let Some((server_tick, frozen_reader)) = self.jitter_buffer.pop_item(receiving_tick) {
            let mut reader = frozen_reader.unfreeze();
            while reader.has_more() {
                let manager_type = ManagerType::de(&mut reader).unwrap();
                match manager_type {
                    ManagerType::Message => {
                        self.base
                            .message_manager
                            .process_message_data(&mut reader, manifest);
                    }
                    ManagerType::Entity => {
                        self.entity_manager.process_data(
                            world,
                            manifest,
                            server_tick,
                            &mut reader,
                            incoming_events,
                        );
                    }
                    _ => {}
                }
            }
        }
    }

    // Outgoing data

    pub fn outgoing_packet(&mut self, client_tick: u16) -> Option<BitWriter> {
        if self.base.message_manager.has_outgoing_messages()
            || self.entity_manager.message_sender.has_outgoing_messages()
        {
            let next_packet_index = self.base.next_packet_index();

            let mut writer = BitWriter::new();

            // Add header
            self.base.write_outgoing_header(
                client_tick,
                PacketType::Data,
                &mut writer,
            );

            // Write Entity Messages
            self.entity_manager.write_messages(&mut writer, next_packet_index);

            // Write Messages
            self.base.message_manager.write_messages(&mut writer, next_packet_index);

            // Return Writer
            return Some(writer);
        }

        return None;
    }
}
