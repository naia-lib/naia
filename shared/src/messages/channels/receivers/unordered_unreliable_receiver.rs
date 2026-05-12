use std::{collections::VecDeque, mem};

use log::{info, warn};

use naia_serde::{BitReader, Serde, SerdeErr};
use naia_socket_shared::Instant;

use crate::world::local::local_world_manager::LocalWorldManager;
use crate::{
    messages::{
        channels::{
            receivers::channel_receiver::{ChannelReceiver, MessageChannelReceiver, RequestsAndResponses},
        },
        message_kinds::MessageKinds,
    },
    world::remote::remote_entity_waitlist::{RemoteEntityWaitlist, WaitlistStore},
    LocalEntityAndGlobalEntityConverter, MessageContainer,
};

pub struct UnorderedUnreliableReceiver {
    incoming_messages: VecDeque<MessageContainer>,
    waitlist_store: WaitlistStore<MessageContainer>,
}

impl UnorderedUnreliableReceiver {
    pub fn new() -> Self {
        Self {
            incoming_messages: VecDeque::new(),
            waitlist_store: WaitlistStore::new(),
        }
    }

    fn read_message(
        &mut self,
        message_kinds: &MessageKinds,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
        reader: &mut BitReader,
    ) -> Result<MessageContainer, SerdeErr> {
        // read payload
        message_kinds.read(reader, converter)
    }

    fn recv_message(
        &mut self,
        local_world_manager: &mut LocalWorldManager,
        message: MessageContainer,
    ) {
        if let Some(remote_entity_set) = message.relations_waiting() {
            warn!(
                "Queuing waiting message {:?}! Waiting on entities: {:?}",
                message.name(),
                remote_entity_set
            );
            local_world_manager.entity_waitlist_queue(
                &remote_entity_set,
                &mut self.waitlist_store,
                message,
            );
            return;
        } else {
            info!("Received message {:?}!", message.name());
        }

        self.incoming_messages.push_back(message);
    }
}

impl ChannelReceiver<MessageContainer> for UnorderedUnreliableReceiver {
    fn receive_messages(
        &mut self,
        _message_kinds: &MessageKinds,
        now: &Instant,
        entity_waitlist: &mut RemoteEntityWaitlist,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
    ) -> Vec<MessageContainer> {
        if let Some(list) = entity_waitlist.collect_ready_items(now, &mut self.waitlist_store) {
            for mut message in list {
                message.relations_complete(converter);
                self.incoming_messages.push_back(message);
            }
        }

        Vec::from(mem::take(&mut self.incoming_messages))
    }
}

impl MessageChannelReceiver for UnorderedUnreliableReceiver {
    fn read_messages(
        &mut self,
        message_kinds: &MessageKinds,
        local_world_manager: &mut LocalWorldManager,
        reader: &mut BitReader,
    ) -> Result<(), SerdeErr> {
        loop {
            let channel_continue = bool::de(reader)?;
            if !channel_continue {
                break;
            }

            let message = self.read_message(
                message_kinds,
                local_world_manager.entity_converter(),
                reader,
            )?;
            self.recv_message(local_world_manager, message);
        }

        Ok(())
    }

    fn receive_requests_and_responses(
        &mut self,
    ) -> RequestsAndResponses {
        panic!("UnorderedUnreliable channels do not support requests");
    }
}
