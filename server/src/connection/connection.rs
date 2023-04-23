use std::{hash::Hash, net::SocketAddr};

use log::warn;

use naia_shared::{
    BaseConnection, BigMapKey, BitReader, BitWriter, ChannelKinds, ConnectionConfig,
    EntityConverter, EntityEvent, HostType, HostWorldEvents, Instant, PacketType, Protocol, Serde,
    SerdeErr, StandardHeader, Tick, WorldMutType, WorldRefType,
};

use crate::{
    connection::{
        io::Io, ping_config::PingConfig, tick_buffer_messages::TickBufferMessages,
        tick_buffer_receiver::TickBufferReceiver,
    },
    events::Events,
    time_manager::TimeManager,
    user::UserKey,
    world::global_world_manager::GlobalWorldManager,
};

use super::ping_manager::PingManager;

pub struct Connection<E: Copy + Eq + Hash + Send + Sync> {
    pub address: SocketAddr,
    pub user_key: UserKey,
    pub base: BaseConnection<E>,
    pub ping_manager: PingManager,
    tick_buffer: TickBufferReceiver,
}

impl<E: Copy + Eq + Hash + Send + Sync> Connection<E> {
    pub fn new(
        connection_config: &ConnectionConfig,
        ping_config: &PingConfig,
        user_address: &SocketAddr,
        user_key: &UserKey,
        channel_kinds: &ChannelKinds,
        global_world_manager: &GlobalWorldManager<E>,
    ) -> Self {
        Connection {
            address: *user_address,
            user_key: *user_key,
            base: BaseConnection::new(
                &Some(*user_address),
                HostType::Server,
                user_key.to_u64(),
                connection_config,
                channel_kinds,
                global_world_manager,
            ),
            tick_buffer: TickBufferReceiver::new(channel_kinds),
            ping_manager: PingManager::new(ping_config),
        }
    }

    pub fn user_key(&self) -> UserKey {
        self.user_key
    }

    // Incoming Data

    pub fn process_incoming_header(&mut self, header: &StandardHeader) {
        self.base.process_incoming_header(header, &mut []);
    }

    /// Read packet data received from a client, storing necessary data in an internal buffer
    pub fn read_packet(
        &mut self,
        protocol: &Protocol,
        server_tick: Tick,
        client_tick: Tick,
        reader: &mut BitReader,
        global_world_manager: &mut GlobalWorldManager<E>,
    ) -> Result<(), SerdeErr> {
        {
            let entity_converter =
                EntityConverter::new(global_world_manager, &self.base.local_world_manager);

            // read tick-buffered messages
            self.tick_buffer.read_messages(
                protocol,
                &server_tick,
                &client_tick,
                &entity_converter,
                reader,
            )?;

            // read messages
            self.base.message_manager.read_messages(
                protocol,
                &mut self.base.remote_world_manager.entity_waitlist,
                &entity_converter,
                reader,
            )?;
        }

        // read world events
        if protocol.client_authoritative_entities {
            self.base.remote_world_reader.read_world_events(
                global_world_manager,
                &mut self.base.local_world_manager,
                protocol,
                client_tick,
                reader,
            )?;
        }

        return Ok(());
    }

    /// Receive & process stored packet data
    pub fn process_packets<W: WorldMutType<E>>(
        &mut self,
        protocol: &Protocol,
        global_world_manager: &mut GlobalWorldManager<E>,
        world: &mut W,
        incoming_events: &mut Events<E>,
    ) {
        // Receive Message Events
        let messages = self.base.message_manager.receive_messages(
            global_world_manager,
            &self.base.local_world_manager,
            &mut self.base.remote_world_manager.entity_waitlist,
        );
        for (channel_kind, messages) in messages {
            for message in messages {
                incoming_events.push_message(&self.user_key, &channel_kind, message);
            }
        }

        // read world events
        if protocol.client_authoritative_entities {
            let remote_events = self.base.remote_world_reader.take_incoming_events();
            let world_events = self.base.remote_world_manager.process_world_events(
                global_world_manager,
                &mut self.base.local_world_manager,
                &protocol.component_kinds,
                world,
                remote_events,
            );
            for event in &world_events {
                if let EntityEvent::SpawnEntity(entity) = event {
                    global_world_manager.remote_spawn_entity_record(entity, &self.user_key);
                }
            }
            incoming_events.receive_entity_events(&self.user_key, world_events);
        }
    }

    pub fn tick_buffer_messages(&mut self, tick: &Tick, messages: &mut TickBufferMessages) {
        let channel_messages = self.tick_buffer.receive_messages(tick);
        for (channel_kind, received_messages) in channel_messages {
            for message in received_messages {
                messages.push_message(&self.user_key, &channel_kind, message);
            }
        }
    }

    // Outgoing data
    pub fn send_outgoing_packets<W: WorldRefType<E>>(
        &mut self,
        protocol: &Protocol,
        now: &Instant,
        io: &mut Io,
        world: &W,
        global_world_manager: &GlobalWorldManager<E>,
        time_manager: &TimeManager,
    ) {
        let rtt_millis = self.ping_manager.rtt_average;
        self.base.collect_outgoing_messages(now, &rtt_millis);
        let mut host_world_events = self
            .base
            .host_world_manager
            .take_outgoing_events(now, &rtt_millis);

        let mut any_sent = false;
        loop {
            if self.send_outgoing_packet(
                protocol,
                now,
                io,
                world,
                global_world_manager,
                time_manager,
                &mut host_world_events,
            ) {
                any_sent = true;
            } else {
                break;
            }
        }
        if any_sent {
            self.base.mark_sent();
        }
    }

    /// Send any message, component actions and component updates to the client
    /// Will split the data into multiple packets.
    fn send_outgoing_packet<W: WorldRefType<E>>(
        &mut self,
        protocol: &Protocol,
        now: &Instant,
        io: &mut Io,
        world: &W,
        global_world_manager: &GlobalWorldManager<E>,
        time_manager: &TimeManager,
        host_world_events: &mut HostWorldEvents<E>,
    ) -> bool {
        if host_world_events.has_events() || self.base.message_manager.has_outgoing_messages() {
            let next_packet_index = self.base.next_packet_index();

            let mut writer = BitWriter::new();

            // Reserve bits we know will be required to finish the message:
            // 1. Messages finish bit
            // 2. Updates finish bit
            // 3. Actions finish bit
            writer.reserve_bits(3);

            // write header
            self.base
                .write_outgoing_header(PacketType::Data, &mut writer);

            // write server tick
            time_manager.current_tick().ser(&mut writer);

            // write server tick instant
            time_manager.current_tick_instant().ser(&mut writer);

            // write common data packet
            let mut has_written = false;

            self.base.write_outgoing_packet(
                &protocol,
                now,
                &mut writer,
                next_packet_index,
                world,
                global_world_manager,
                &mut has_written,
                true,
                host_world_events,
            );

            // send packet
            if io.send_packet(&self.address, writer.to_packet()).is_err() {
                // TODO: pass this on and handle above
                warn!("Server Error: Cannot send data packet to {}", &self.address);
            }

            return true;
        }

        false
    }
}
