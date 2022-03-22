use std::collections::{HashMap, VecDeque};

use naia_shared::{read_list_header, sequence_greater_than, serde::{BitReader, Serde}, Manifest, Protocolize, Tick, NetEntityHandleConverter, ChannelIndex};

type ShortMsgId = u8;

/// Handles incoming Tick Buffered Messages
pub struct TickBufferMessageReceiver<P: Protocolize, C: ChannelIndex> {
    incoming_messages: IncomingMessages<P, C>,
}

impl<P: Protocolize, C: ChannelIndex> TickBufferMessageReceiver<P, C> {
    /// Creates a new TickBufferMessageReceiver
    pub fn new() -> Self {
        TickBufferMessageReceiver {
            incoming_messages: IncomingMessages::new(),
        }
    }

    /// Get the most recently received Message
    pub fn pop_incoming_message(&mut self, server_tick: Tick) -> Option<(C, P)> {
        return self.incoming_messages.pop_front(server_tick);
    }

    pub fn read_messages(
        &mut self,
        server_tick: Tick,
        reader: &mut BitReader,
        manifest: &Manifest<P>,
        converter: &mut dyn NetEntityHandleConverter,
    ) {
        let message_count = read_list_header(reader);
        self.process_incoming_messages(server_tick, reader, manifest, message_count, converter);
    }

    /// Given incoming packet data, read transmitted Message and store
    /// them to be returned to the application
    fn process_incoming_messages(
        &mut self,
        server_tick: Tick,
        reader: &mut BitReader,
        manifest: &Manifest<P>,
        message_count: u16,
        converter: &dyn NetEntityHandleConverter,
    ) {
        for _x in 0..message_count {
            // read client tick
            let client_tick = Tick::de(reader).unwrap();

            // read message id
            let short_msg_id: ShortMsgId = ShortMsgId::de(reader).unwrap();

            // read message channel
            let channel: C = C::de(reader).unwrap();

            // read message kind
            let replica_kind: P::Kind = P::Kind::de(reader).unwrap();

            // read payload
            let new_message = manifest.create_replica(replica_kind, reader, converter);

            if !self.incoming_messages.push_back(
                client_tick,
                server_tick,
                short_msg_id,
                channel,
                new_message,
            ) {
                //info!("failed command. server: {}, client: {}",
                // server_tick, client_tick);
            }
        }
    }
}

// Incoming messages

struct IncomingMessages<P: Protocolize, C: ChannelIndex> {
    // front is small, back is big
    buffer: VecDeque<(Tick, HashMap<ShortMsgId, (C, P)>)>,
}

impl<P: Protocolize, C: ChannelIndex> IncomingMessages<P, C> {
    pub fn new() -> Self {
        IncomingMessages {
            buffer: VecDeque::new(),
        }
    }

    pub fn push_back(
        &mut self,
        client_tick: Tick,
        server_tick: Tick,
        short_msg_id: ShortMsgId,
        message_channel: C,
        new_message: P,
    ) -> bool {
        if sequence_greater_than(client_tick, server_tick) {
            let mut index = self.buffer.len();

            //in the case of empty vec
            if index == 0 {
                let mut map = HashMap::new();
                map.insert(short_msg_id, (message_channel, new_message));
                self.buffer.push_back((client_tick, map));
                //info!("msg server_tick: {}, client_tick: {}, for entity: {} ... (empty q)",
                // server_tick, client_tick, owned_entity);
                return true;
            }

            let mut insert = false;
            loop {
                index -= 1;

                if let Some((tick, command_map)) = self.buffer.get_mut(index) {
                    if *tick == client_tick {
                        if !command_map.contains_key(&short_msg_id) {
                            command_map.insert(short_msg_id, (message_channel, new_message));
                            //info!("inserting command at tick: {}", client_tick);
                            //info!("msg server_tick: {}, client_tick: {}, for entity: {} ... (map
                            // xzist)", server_tick, client_tick, owned_entity);
                            // inserted command into existing tick
                            return true;
                        } else {
                            return false;
                        }
                    } else {
                        if sequence_greater_than(client_tick, *tick) {
                            // incoming client tick is larger than found tick ...
                            insert = true;
                        }
                    }
                }

                if insert {
                    // found correct position to insert node
                    let mut map = HashMap::new();
                    map.insert(short_msg_id, (message_channel, new_message));
                    self.buffer.insert(index + 1, (client_tick, map));
                    //info!("msg server_tick: {}, client_tick: {}, for entity: {} ... (midbck
                    // insrt)", server_tick, client_tick, owned_entity);
                    return true;
                }

                if index == 0 {
                    //traversed the whole vec, push front
                    let mut map = HashMap::new();
                    map.insert(short_msg_id, (message_channel, new_message));
                    self.buffer.push_front((client_tick, map));
                    //info!("msg server_tick: {}, client_tick: {}, for entity: {} ... (front
                    // insrt)", server_tick, client_tick, owned_entity);
                    return true;
                }
            }
        } else {
            // command is too late to insert in incoming message queue
            return false;
        }
    }

    pub fn pop_front(&mut self, server_tick: Tick) -> Option<(C, P)> {
        // get rid of outdated commands
        loop {
            let mut pop = false;
            if let Some((front_tick, _)) = self.buffer.front() {
                if sequence_greater_than(server_tick, *front_tick) {
                    pop = true;
                }
            } else {
                return None;
            }
            if pop {
                self.buffer.pop_front();
            } else {
                break;
            }
        }

        // now get the newest applicable command
        let mut output = None;
        let mut pop = false;
        if let Some((front_tick, command_map)) = self.buffer.front_mut() {
            if *front_tick == server_tick {
                let mut any_msg_id: Option<ShortMsgId> = None;
                if let Some(any_msg_id_ref) = command_map.keys().next() {
                    any_msg_id = Some(*any_msg_id_ref);
                }
                if let Some(msg_id) = any_msg_id {
                    if let Some(message) = command_map.remove(&msg_id) {
                        output = Some(message);
                        // info!("popping message at tick: {}, for entity: {}",
                        // front_tick, any_entity);
                    }
                    if command_map.len() == 0 {
                        pop = true;
                    }
                }
            }
        }

        if pop {
            self.buffer.pop_front();
        }

        return output;
    }
}
