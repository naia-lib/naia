use std::{any::Any, hash::Hash};

use log::warn;

use naia_shared::{
    BaseConnection, BitReader, BitWriter, ChannelKind, ChannelKinds, ComponentKinds,
    ConnectionConfig, EntityEventMessage, EntityResponseEvent, HostType,
    HostWorldEvents, Instant, OwnedBitReader, PacketType, Protocol, Serde, SerdeErr,
    StandardHeader, SystemChannel, Tick, WorldMutType, WorldRefType,
};

use crate::{
    connection::{
        io::Io, tick_buffer_sender::TickBufferSender, tick_queue::TickQueue,
        time_manager::TimeManager,
    },
    events::Events,
    world::global_world_manager::GlobalWorldManager,
};

pub struct Connection<E: Copy + Eq + Hash + Send + Sync> {
    pub base: BaseConnection<E>,
    pub time_manager: TimeManager,
    pub tick_buffer: TickBufferSender,
    /// Small buffer when receiving updates (entity actions, entity updates) from the server
    /// to make sure we receive them in order
    jitter_buffer: TickQueue<OwnedBitReader>,
}

impl<E: Copy + Eq + Hash + Send + Sync> Connection<E> {
    pub fn new(
        connection_config: &ConnectionConfig,
        channel_kinds: &ChannelKinds,
        time_manager: TimeManager,
        global_world_manager: &GlobalWorldManager<E>,
    ) -> Self {
        let tick_buffer = TickBufferSender::new(channel_kinds);

        let mut connection = Connection {
            base: BaseConnection::new(
                &None,
                HostType::Client,
                0,
                connection_config,
                channel_kinds,
                global_world_manager,
            ),
            time_manager,
            tick_buffer,
            jitter_buffer: TickQueue::new(),
        };

        let existing_entities = global_world_manager.entities();
        for entity in existing_entities {
            let component_kinds = global_world_manager.component_kinds(&entity).unwrap();
            connection.base.host_world_manager.init_entity(
                &mut connection.base.local_world_manager,
                &entity,
                component_kinds,
            );
        }

        connection
    }

    // Incoming data

    pub fn process_incoming_header(&mut self, header: &StandardHeader) {
        self.base
            .process_incoming_header(header, &mut [&mut self.tick_buffer]);
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
    /// `receiving_tick`. Reads packets, storing necessary data into an internal buffer
    pub fn read_buffered_packets(
        &mut self,
        protocol: &Protocol,
        global_world_manager: &mut GlobalWorldManager<E>,
    ) -> Result<(), SerdeErr> {
        let receiving_tick = self.time_manager.client_receiving_tick;

        while let Some((server_tick, owned_reader)) = self.jitter_buffer.pop_item(receiving_tick) {
            let mut reader = owned_reader.borrow();

            self.base.read_packet(
                protocol,
                &server_tick,
                global_world_manager,
                true,
                &mut reader,
            )?;
        }

        Ok(())
    }

    /// Receive & process messages / entity actions / entity updates and emit events for them
    pub fn process_packets<W: WorldMutType<E>>(
        &mut self,
        global_world_manager: &mut GlobalWorldManager<E>,
        component_kinds: &ComponentKinds,
        world: &mut W,
        incoming_events: &mut Events<E>,
    ) -> Vec<EntityResponseEvent<E>> {
        let mut response_events = Vec::new();
        // Receive Message Events
        let messages = self.base.message_manager.receive_messages(
            global_world_manager,
            &self.base.local_world_manager,
            &mut self.base.remote_world_manager.entity_waitlist,
        );
        for (channel_kind, messages) in messages {
            if channel_kind == ChannelKind::of::<SystemChannel>() {
                for message in messages {
                    let Some(event_message) = Box::<dyn Any + 'static>::downcast::<EntityEventMessage>(message.to_boxed_any())
                        .ok()
                        .map(|boxed_m| *boxed_m) else {
                        panic!("Received unknown message over SystemChannel!");
                    };
                    let Some(entity) = event_message.entity.get(global_world_manager) else {
                        panic!("Received message with no Entity over SystemChannel!");
                    };
                    response_events.push(event_message.action.to_response_event(&entity));
                }
            } else {
                for message in messages {
                    incoming_events.push_message(&channel_kind, message);
                }
            }
        }

        // Receive World Events
        let remote_events = self.base.remote_world_reader.take_incoming_events();
        let world_events = self.base.remote_world_manager.process_world_events(
            global_world_manager,
            &mut self.base.local_world_manager,
            component_kinds,
            world,
            remote_events,
        );
        response_events.extend(incoming_events.receive_world_events(world_events));
        response_events
    }

    // Outgoing data

    /// Collect and send any outgoing packets from client to server
    pub fn send_packets<W: WorldRefType<E>>(
        &mut self,
        protocol: &Protocol,
        now: &Instant,
        io: &mut Io,
        world: &W,
        global_world_manager: &GlobalWorldManager<E>,
    ) {
        let rtt_millis = self.time_manager.rtt();
        self.base.collect_messages(now, &rtt_millis);
        self.tick_buffer.collect_messages(
            &self.time_manager.client_sending_tick,
            &self.time_manager.server_receivable_tick,
        );
        let mut host_world_events = self
            .base
            .host_world_manager
            .take_outgoing_events(now, &rtt_millis);

        let mut any_sent = false;
        loop {
            if self.send_packet(
                protocol,
                now,
                io,
                world,
                global_world_manager,
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

    // Sends packet and returns whether or not a packet was sent
    fn send_packet<W: WorldRefType<E>>(
        &mut self,
        protocol: &Protocol,
        now: &Instant,
        io: &mut Io,
        world: &W,
        global_world_manager: &GlobalWorldManager<E>,
        host_world_events: &mut HostWorldEvents<E>,
    ) -> bool {
        if host_world_events.has_events()
            || self.base.message_manager.has_outgoing_messages()
            || self.tick_buffer.has_messages()
        {
            let writer = self.write_packet(
                protocol,
                now,
                world,
                global_world_manager,
                host_world_events,
            );

            // send packet
            if io.send_packet(writer.to_packet()).is_err() {
                // TODO: pass this on and handle above
                warn!("Client Error: Cannot send data packet to Server");
            }

            return true;
        }

        false
    }

    fn write_packet<W: WorldRefType<E>>(
        &mut self,
        protocol: &Protocol,
        now: &Instant,
        world: &W,
        global_world_manager: &GlobalWorldManager<E>,
        host_world_events: &mut HostWorldEvents<E>,
    ) -> BitWriter {
        let next_packet_index = self.base.next_packet_index();

        let mut writer = BitWriter::new();

        // Reserve bits we know will be required to finish the message:
        // 1. Tick buffer finish bit
        // 2. Messages finish bit
        // 3. Updates finish bit
        // 4. Actions finish bit
        writer.reserve_bits(4);

        // write header
        self.base.write_header(PacketType::Data, &mut writer);

        // write client tick
        let client_tick: Tick = self.time_manager.client_sending_tick;
        client_tick.ser(&mut writer);

        let mut has_written = false;

        // write tick buffered messages
        self.tick_buffer.write_messages(
            &protocol,
            global_world_manager,
            &mut self.base.local_world_manager,
            &mut writer,
            next_packet_index,
            &client_tick,
            &mut has_written,
        );

        // write common parts of packet (messages & world events)
        self.base.write_packet(
            protocol,
            now,
            &mut writer,
            next_packet_index,
            world,
            global_world_manager,
            &mut has_written,
            protocol.client_authoritative_entities,
            host_world_events,
        );

        writer
    }
}
