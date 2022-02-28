use std::collections::{HashMap, VecDeque};

use naia_shared::{
    sequence_greater_than, Manifest, NetEntity, ProtocolKindType,
    Protocolize,
};
use naia_shared::serde::{BitReader, Serde};

/// Handles incoming Entity Messages, buffering them to be received on the
/// correct tick
pub struct EntityMessageReceiver<P: Protocolize> {
    incoming_messages: IncomingMessages<P>,
}

impl<P: Protocolize> EntityMessageReceiver<P> {
    /// Creates a new EntityMessageReceiver
    pub fn new() -> Self {
        EntityMessageReceiver {
            incoming_messages: IncomingMessages::new(),
        }
    }

    /// Get the most recently received Entity Message
    pub fn pop_incoming_entity_message(&mut self, server_tick: u16) -> Option<(NetEntity, P)> {
        return self.incoming_messages.pop_front(server_tick);
    }

    /// Given incoming packet data, read transmitted Entity Message and store
    /// them to be returned to the application
    pub fn process_incoming_messages(
        &mut self,
        server_tick_opt: Option<Tick>,
        reader: &mut BitReader,
        manifest: &Manifest<P>,
    ) {
        let message_count = u8::de(reader).unwrap();
        for _x in 0..message_count {
            let client_tick = u16::de(reader).unwrap();
            let owned_entity = NetEntity::de(reader).unwrap();
            let replica_kind: P::Kind = P::Kind::de(reader).unwrap();

            let new_message = manifest.create_replica(replica_kind, reader);

            if let Some(server_tick) = server_tick_opt {
                if !self.incoming_messages.push_back(
                    client_tick,
                    server_tick,
                    owned_entity,
                    new_message,
                ) {
                    //info!("failed command. server: {}, client: {}",
                    // server_tick, client_tick);
                } else {
                }
            }
        }
    }
}

// Incoming messages

type Tick = u16;

struct IncomingMessages<P> {
    // front is small, back is big
    buffer: VecDeque<(Tick, HashMap<NetEntity, P>)>,
}

impl<P> IncomingMessages<P> {
    pub fn new() -> Self {
        IncomingMessages {
            buffer: VecDeque::new(),
        }
    }

    pub fn push_back(
        &mut self,
        client_tick: u16,
        server_tick: u16,
        owned_entity: NetEntity,
        new_message: P,
    ) -> bool {
        if sequence_greater_than(client_tick, server_tick) {
            let mut index = self.buffer.len();

            //in the case of empty vec
            if index == 0 {
                let mut map = HashMap::new();
                map.insert(owned_entity, new_message);
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
                        if !command_map.contains_key(&owned_entity) {
                            command_map.insert(owned_entity, new_message);
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
                    map.insert(owned_entity, new_message);
                    self.buffer.insert(index + 1, (client_tick, map));
                    //info!("msg server_tick: {}, client_tick: {}, for entity: {} ... (midbck
                    // insrt)", server_tick, client_tick, owned_entity);
                    return true;
                }

                if index == 0 {
                    //traversed the whole vec, push front
                    let mut map = HashMap::new();
                    map.insert(owned_entity, new_message);
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

    pub fn pop_front(&mut self, server_tick: u16) -> Option<(NetEntity, P)> {
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
                let mut any_entity: Option<NetEntity> = None;
                if let Some(any_entity_ref) = command_map.keys().next() {
                    any_entity = Some(*any_entity_ref);
                }
                if let Some(any_entity) = any_entity {
                    if let Some(message) = command_map.remove(&any_entity) {
                        output = Some((any_entity, message));
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
