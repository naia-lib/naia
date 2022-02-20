use std::{collections::VecDeque, hash::Hash, net::SocketAddr};

use naia_shared::{
    BaseConnection, ConnectionConfig, ManagerType, Manifest, MonitorConfig, PacketReader,
    PacketType, PacketWriteState, Protocolize, StandardHeader, Tick, WorldMutType,
};

use super::{
    entity_manager::EntityManager, error::NaiaClientError, event::Event, ping_manager::PingManager,
    tick_manager::TickManager, tick_queue::TickQueue,
};

pub struct Connection<P: Protocolize, E: Copy + Eq + Hash> {
    pub base: BaseConnection<P>,
    pub entity_manager: EntityManager<P, E>,
    pub ping_manager: Option<PingManager>,
    jitter_buffer: TickQueue<Box<[u8]>>,
}

impl<P: Protocolize, E: Copy + Eq + Hash> Connection<P, E> {
    pub fn new(
        address: SocketAddr,
        connection_config: &ConnectionConfig,
        monitor_config: &Option<MonitorConfig>,
    ) -> Self {
        let ping_manager: Option<PingManager> = monitor_config.as_ref().map(|config| {
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

    pub fn buffer_data_packet(&mut self, incoming_tick: u16, incoming_payload: &Box<[u8]>) {
        self.jitter_buffer
            .add_item(incoming_tick, incoming_payload.clone());
    }

    pub fn process_buffered_packets<W: WorldMutType<P, E>>(
        &mut self,
        world: &mut W,
        manifest: &Manifest<P>,
        receiving_tick: Tick,
        incoming_events: &mut VecDeque<Result<Event<P, E>, NaiaClientError>>,
    ) {
        while let Some((server_tick, data_packet)) = self.jitter_buffer.pop_item(receiving_tick) {
            let mut reader = PacketReader::new(&data_packet);
            while reader.has_more() {
                let manager_type: ManagerType = reader.read_u8().into();
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

    pub fn outgoing_packet(&mut self, client_tick: u16) -> Option<Box<[u8]>> {
        if self.base.message_manager.has_outgoing_messages()
            || self.entity_manager.message_sender.has_outgoing_messages()
        {
            let mut write_state = PacketWriteState::new(self.base.next_packet_index());

            // Write Entity Messages
            self.entity_manager.queue_writes(&mut write_state);

            // Write Messages
            self.base.message_manager.queue_writes(&mut write_state);

            // Add header
            if write_state.has_bytes() {
                // Get bytes from writer
                let mut out_bytes = Vec::<u8>::new();
                self.base.message_manager.flush_writes(&mut out_bytes);
                self.entity_manager.flush_writes(&mut out_bytes);

                // Add header to it
                let payload = self.base.process_outgoing_header(
                    client_tick,
                    PacketType::Data,
                    &out_bytes.into_boxed_slice(),
                );
                return Some(payload);
            } else {
                panic!("Pending outgoing messages but no bytes were written... Likely trying to transmit a Component/Message larger than 576 bytes!");
            }
        }

        return None;
    }
}
