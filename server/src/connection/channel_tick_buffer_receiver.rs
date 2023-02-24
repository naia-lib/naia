use std::collections::{HashMap, VecDeque};

use naia_shared::{
    sequence_greater_than, BitReader, ChannelReader, Message, MessageKinds, Serde, SerdeErr,
    ShortMessageIndex, Tick, TickBufferSettings, UnsignedVariableInteger,
};

/// Receive updates from the client and store them in a buffer along with the corresponding
/// client tick.
pub struct ChannelTickBufferReceiver {
    incoming_messages: IncomingMessages,
}

impl ChannelTickBufferReceiver {
    pub fn new(_settings: TickBufferSettings) -> Self {
        Self {
            incoming_messages: IncomingMessages::new(),
        }
    }

    /// Read the stored buffer-data corresponding to the given [`Tick`]
    pub fn receive_messages(&mut self, host_tick: &Tick) -> Vec<Box<dyn Message>> {
        self.incoming_messages.collect(host_tick)
    }

    /// Given incoming packet data, read transmitted Messages and store
    /// them in a buffer to be returned to the application
    pub fn read_messages(
        &mut self,
        message_kinds: &MessageKinds,
        host_tick: &Tick,
        remote_tick: &Tick,
        channel_reader: &dyn ChannelReader<Box<dyn Message>>,
        reader: &mut BitReader,
    ) -> Result<(), SerdeErr> {
        let mut last_read_tick = *remote_tick;

        loop {
            let message_continue = bool::de(reader)?;
            if !message_continue {
                break;
            }

            self.read_message(
                message_kinds,
                host_tick,
                &mut last_read_tick,
                channel_reader,
                reader,
            )?;
        }

        Ok(())
    }

    /// Given incoming packet data, read transmitted Message and store
    /// them to be returned to the application
    fn read_message(
        &mut self,
        message_kinds: &MessageKinds,
        host_tick: &Tick,
        last_read_tick: &mut Tick,
        channel_reader: &dyn ChannelReader<Box<dyn Message>>,
        reader: &mut BitReader,
    ) -> Result<(), SerdeErr> {
        // read remote tick
        let remote_tick_diff = UnsignedVariableInteger::<3>::de(reader)?.get() as Tick;
        *last_read_tick = last_read_tick.wrapping_sub(remote_tick_diff);
        let remote_tick = *last_read_tick;

        // read message count
        let message_count = UnsignedVariableInteger::<3>::de(reader)?.get();

        let mut last_read_message_index: ShortMessageIndex = 0;
        for _ in 0..message_count {
            // read message id diff, add to last read id
            let id_diff = UnsignedVariableInteger::<2>::de(reader)?.get() as ShortMessageIndex;
            let message_index: ShortMessageIndex = last_read_message_index + id_diff;
            last_read_message_index = message_index;

            // read payload
            let new_message = channel_reader.read(message_kinds, reader)?;

            if !self
                .incoming_messages
                .insert(host_tick, &remote_tick, message_index, new_message)
            {
                // Failed to Insert Command
            }
        }

        Ok(())
    }
}

// Incoming messages

struct IncomingMessages {
    // front is small, back is big
    // front is present, back is future
    /// Buffer containing messages from the client, along with the corresponding tick
    /// We do not store anything for empty ticks
    buffer: VecDeque<(Tick, HashMap<ShortMessageIndex, Box<dyn Message>>)>,
}

impl IncomingMessages {
    pub fn new() -> Self {
        IncomingMessages {
            buffer: VecDeque::new(),
        }
    }

    /// Insert a message from the client into the tick-buffer
    /// Will only insert messages that are from future ticks compared to the current server tick
    pub fn insert(
        &mut self,
        host_tick: &Tick,
        message_tick: &Tick,
        message_index: ShortMessageIndex,
        new_message: Box<dyn Message>,
    ) -> bool {
        // TODO:
        //  * add unit test?
        //  * should there be a maximum buffer size?

        if sequence_greater_than(*message_tick, *host_tick) {
            let mut index = self.buffer.len();

            //in the case of empty vec
            if index == 0 {
                let mut map = HashMap::new();
                map.insert(message_index, new_message);
                self.buffer.push_back((*message_tick, map));
                return true;
            }

            let mut insert = false;

            // loop from back to front (future to present)
            loop {
                index -= 1;

                if let Some((existing_tick, existing_messages)) = self.buffer.get_mut(index) {
                    if *existing_tick == *message_tick {
                        // should almost never collide
                        if let std::collections::hash_map::Entry::Vacant(e) =
                            existing_messages.entry(message_index)
                        {
                            e.insert(new_message);

                            return true;
                        } else {
                            // TODO: log hash collisions?
                            return false;
                        }
                    } else if sequence_greater_than(*message_tick, *existing_tick) {
                        // incoming client tick is larger (more in the future) than found tick
                        insert = true;
                    }
                }

                if insert {
                    // found correct position to insert node
                    let mut new_messages = HashMap::new();
                    new_messages.insert(message_index, new_message);
                    self.buffer.insert(index + 1, (*message_tick, new_messages));
                    return true;
                }

                if index == 0 {
                    //traversed the whole vec, push front
                    let mut new_messages = HashMap::new();
                    new_messages.insert(message_index, new_message);
                    self.buffer.push_front((*message_tick, new_messages));
                    return true;
                }
            }
        } else {
            // command is too late to insert in incoming message queue
            false
        }
    }

    /// Delete from the buffer all data that is older than the provided [`Tick`]
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

    /// Retrieve from the buffer data corresponding to the provided [`Tick`]
    pub fn collect(&mut self, host_tick: &Tick) -> Vec<Box<dyn Message>> {
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
