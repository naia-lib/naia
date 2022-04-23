use std::collections::{HashMap, VecDeque};

use naia_shared::{
    message_list_header, sequence_greater_than,
    serde::{BitReader, Serde, UnsignedVariableInteger},
    ChannelReader, Protocolize, ShortMessageId, Tick,
};

pub struct ChannelTickBufferReceiver<P: Protocolize> {
    incoming_messages: IncomingMessages<P>,
}

impl<P: Protocolize> ChannelTickBufferReceiver<P> {
    pub fn new() -> Self {
        Self {
            incoming_messages: IncomingMessages::new(),
        }
    }

    pub fn receive_messages(&mut self, host_tick: &Tick) -> Vec<P> {
        self.incoming_messages.collect(host_tick)
    }

    pub fn read_messages(
        &mut self,
        host_tick: &Tick,
        remote_tick: &Tick,
        channel_reader: &dyn ChannelReader<P>,
        bit_reader: &mut BitReader,
    ) {
        let mut last_read_tick = *remote_tick;
        let message_count = message_list_header::read(bit_reader);
        for _ in 0..message_count {
            self.read_message(host_tick, &mut last_read_tick, channel_reader, bit_reader);
        }
    }

    /// Given incoming packet data, read transmitted Message and store
    /// them to be returned to the application
    fn read_message(
        &mut self,
        host_tick: &Tick,
        last_read_tick: &mut Tick,
        channel_reader: &dyn ChannelReader<P>,
        bit_reader: &mut BitReader,
    ) {
        // read remote tick
        let remote_tick_diff = UnsignedVariableInteger::<3>::de(bit_reader).unwrap().get() as Tick;
        *last_read_tick = last_read_tick.wrapping_sub(remote_tick_diff);
        let remote_tick = *last_read_tick;

        // read message count
        let message_count = UnsignedVariableInteger::<3>::de(bit_reader).unwrap().get();

        let mut last_read_message_id: ShortMessageId = 0;
        for _ in 0..message_count {
            // read message id diff, add to last read id
            let id_diff =
                UnsignedVariableInteger::<2>::de(bit_reader).unwrap().get() as ShortMessageId;
            let message_id: ShortMessageId = last_read_message_id + id_diff;
            last_read_message_id = message_id;

            // read payload
            let new_message = channel_reader.read(bit_reader);

            if !self
                .incoming_messages
                .insert(host_tick, &remote_tick, message_id, new_message)
            {
                //info!("failed command. server: {}, client: {}",
                // server_tick, client_tick);
            }
        }
    }
}

// Incoming messages

struct IncomingMessages<P: Protocolize> {
    // front is small, back is big
    // front is present, back is future
    buffer: VecDeque<(Tick, HashMap<ShortMessageId, P>)>,
}

impl<P: Protocolize> IncomingMessages<P> {
    pub fn new() -> Self {
        IncomingMessages {
            buffer: VecDeque::new(),
        }
    }

    pub fn insert(
        &mut self,
        host_tick: &Tick,
        message_tick: &Tick,
        message_id: ShortMessageId,
        new_message: P,
    ) -> bool {
        if sequence_greater_than(*message_tick, *host_tick) {
            let mut index = self.buffer.len();

            //in the case of empty vec
            if index == 0 {
                let mut map = HashMap::new();
                map.insert(message_id, new_message);
                self.buffer.push_back((*message_tick, map));
                //info!("msg server_tick: {}, client_tick: {}, for entity: {} ... (empty q)",
                // server_tick, client_tick, owned_entity);
                return true;
            }

            let mut insert = false;
            loop {
                index -= 1;

                if let Some((existing_tick, existing_messages)) = self.buffer.get_mut(index) {
                    if *existing_tick == *message_tick {
                        if let std::collections::hash_map::Entry::Vacant(e) =
                            existing_messages.entry(message_id)
                        {
                            e.insert(new_message);
                            //info!("inserting command at tick: {}", client_tick);
                            //info!("msg server_tick: {}, client_tick: {}, for entity: {} ... (map
                            // xzist)", server_tick, client_tick, owned_entity);
                            // inserted command into existing tick
                            return true;
                        } else {
                            return false;
                        }
                    } else if sequence_greater_than(*message_tick, *existing_tick) {
                        // incoming client tick is larger than found tick ...
                        insert = true;
                    }
                }

                if insert {
                    // found correct position to insert node
                    let mut new_messages = HashMap::new();
                    new_messages.insert(message_id, new_message);
                    self.buffer.insert(index + 1, (*message_tick, new_messages));
                    //info!("msg server_tick: {}, client_tick: {}, for entity: {} ... (midbck
                    // insrt)", server_tick, client_tick, owned_entity);
                    return true;
                }

                if index == 0 {
                    //traversed the whole vec, push front
                    let mut new_messages = HashMap::new();
                    new_messages.insert(message_id, new_message);
                    self.buffer.push_front((*message_tick, new_messages));
                    //info!("msg server_tick: {}, client_tick: {}, for entity: {} ... (front
                    // insrt)", server_tick, client_tick, owned_entity);
                    return true;
                }
            }
        } else {
            // command is too late to insert in incoming message queue
            false
        }
    }

    fn prune_outdated_commands(&mut self, host_tick: &Tick) {
        loop {
            let mut pop = false;
            if let Some((front_tick, _)) = self.buffer.front() {
                if sequence_greater_than(*host_tick, *front_tick) {
                    pop = true;
                }
            }
            if pop {
                self.buffer.pop_front();
            } else {
                break;
            }
        }
    }

    pub fn collect(&mut self, host_tick: &Tick) -> Vec<P> {
        self.prune_outdated_commands(host_tick);

        // now get the newest applicable command
        let mut output = Vec::new();
        let mut pop = false;
        if let Some((front_tick, _)) = self.buffer.front_mut() {
            if *front_tick == *host_tick {
                pop = true;
            }
        }
        if pop {
            if let Some((_, mut command_map)) = self.buffer.pop_front() {
                for (_, message) in command_map.drain() {
                    output.push(message);
                }
            }
        }

        output
    }
}
