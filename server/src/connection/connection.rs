use std::{
    hash::Hash,
    net::SocketAddr,
    sync::{Arc, RwLock},
};

use log::warn;

use naia_shared::{
    BaseConnection, BitReader, BitWriter, ChannelKinds, ConnectionConfig, EntityConverter,
    HostType, Instant, PacketType, Protocol, Serde, SerdeErr, StandardHeader, Tick, WorldRefType,
};

use crate::{
    connection::{
        ping_config::PingConfig, tick_buffer_messages::TickBufferMessages,
        tick_buffer_receiver::TickBufferReceiver, time_manager::TimeManager,
    },
    protocol::{
        entity_manager::EntityManager, global_diff_handler::GlobalDiffHandler,
        world_record::WorldRecord,
    },
    user::UserKey,
    Events,
};

use super::{io::Io, ping_manager::PingManager};

pub struct Connection<E: Copy + Eq + Hash + Send + Sync> {
    pub user_key: UserKey,
    pub base: BaseConnection,
    pub entity_manager: EntityManager<E>,
    tick_buffer: TickBufferReceiver,
    pub ping_manager: PingManager,
}

impl<E: Copy + Eq + Hash + Send + Sync> Connection<E> {
    pub fn new(
        connection_config: &ConnectionConfig,
        ping_config: &PingConfig,
        user_address: SocketAddr,
        user_key: &UserKey,
        channel_kinds: &ChannelKinds,
        diff_handler: &Arc<RwLock<GlobalDiffHandler<E>>>,
    ) -> Self {
        Connection {
            user_key: *user_key,
            base: BaseConnection::new(
                user_address,
                HostType::Server,
                connection_config,
                channel_kinds,
            ),
            entity_manager: EntityManager::new(user_address, diff_handler),
            tick_buffer: TickBufferReceiver::new(channel_kinds),
            ping_manager: PingManager::new(ping_config),
        }
    }

    // Incoming Data

    pub fn process_incoming_header(&mut self, header: &StandardHeader) {
        self.base
            .process_incoming_header(header, &mut Some(&mut self.entity_manager));
    }

    /// Read packet data received from a client
    pub fn process_incoming_data(
        &mut self,
        protocol: &Protocol,
        server_tick: Tick,
        client_tick: Tick,
        reader: &mut BitReader,
        world_record: &WorldRecord<E>,
    ) -> Result<(), SerdeErr> {
        let converter = EntityConverter::new(world_record, &self.entity_manager);

        // read tick-buffered messages
        {
            self.tick_buffer.read_messages(
                protocol,
                &server_tick,
                &client_tick,
                &converter,
                reader,
            )?;
        }

        // read messages
        {
            self.base
                .message_manager
                .read_messages(protocol, &converter, reader)?;
        }

        Ok(())
    }

    pub fn receive_messages(&mut self, incoming_events: &mut Events) {
        let received_messages = self.base.message_manager.receive_messages();
        for (channel_kind, message) in received_messages {
            incoming_events.push_message(&self.user_key, &channel_kind, message);
        }
    }

    pub fn tick_buffer_messages(&mut self, tick: &Tick, messages: &mut TickBufferMessages) {
        let channel_messages = self.tick_buffer.receive_messages(tick);
        for (channel_kind, received_messages) in channel_messages {
            for message in received_messages {
                messages.push(&self.user_key, &channel_kind, message);
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
        world_record: &WorldRecord<E>,
        time_manager: &TimeManager,
        rtt_millis: &f32,
    ) {
        self.collect_outgoing_messages(now, rtt_millis);

        let mut any_sent = false;
        loop {
            if self.send_outgoing_packet(protocol, now, io, world, world_record, time_manager) {
                any_sent = true;
            } else {
                break;
            }
        }
        if any_sent {
            self.base.mark_sent();
        }
    }

    fn collect_outgoing_messages(&mut self, now: &Instant, rtt_millis: &f32) {
        self.entity_manager.collect_outgoing_messages(
            now,
            rtt_millis,
            &mut self.base.message_manager,
        );
        self.base
            .message_manager
            .collect_outgoing_messages(now, rtt_millis);
    }

    /// Send any message, component actions and component updates to the client
    /// Will split the data into multiple packets.
    fn send_outgoing_packet<W: WorldRefType<E>>(
        &mut self,
        protocol: &Protocol,
        now: &Instant,
        io: &mut Io,
        world: &W,
        world_record: &WorldRecord<E>,
        time_manager: &TimeManager,
    ) -> bool {
        if self.base.message_manager.has_outgoing_messages()
            || self.entity_manager.has_outgoing_messages()
        {
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

            let mut has_written = false;

            // write messages
            {
                let converter = EntityConverter::new(world_record, &self.entity_manager);
                self.base.message_manager.write_messages(
                    &protocol,
                    &converter,
                    &mut writer,
                    next_packet_index,
                    &mut has_written,
                );

                // finish messages
                false.ser(&mut writer);
                writer.release_bits(1);
            }

            // write entity updates
            {
                self.entity_manager.write_updates(
                    &protocol.component_kinds,
                    now,
                    &mut writer,
                    &next_packet_index,
                    world,
                    world_record,
                    &mut has_written,
                );

                // finish updates
                false.ser(&mut writer);
                writer.release_bits(1);
            }

            // write entity actions
            {
                self.entity_manager.write_actions(
                    &protocol.component_kinds,
                    now,
                    &mut writer,
                    &next_packet_index,
                    world,
                    world_record,
                    &mut has_written,
                );

                // finish actions
                false.ser(&mut writer);
                writer.release_bits(1);
            }

            // send packet
            if io
                .send_packet(&self.base.address, writer.to_packet())
                .is_err()
            {
                // TODO: pass this on and handle above
                warn!(
                    "Server Error: Cannot send data packet to {}",
                    &self.base.address
                );
            }

            return true;
        }

        false
    }
}
