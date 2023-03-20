use std::net::SocketAddr;

use naia_shared::{link_condition_logic, LinkConditionerConfig, TimeQueue};

use super::{PacketReceiver, RecvError};

/// Used to receive packets from the Client Socket
#[derive(Clone)]
pub struct ConditionedPacketReceiver {
    inner_receiver: Box<dyn PacketReceiver>,
    link_conditioner_config: LinkConditionerConfig,
    time_queue: TimeQueue<(SocketAddr, Box<[u8]>)>,
    last_payload: Option<Box<[u8]>>,
}

impl ConditionedPacketReceiver {
    /// Creates a new ConditionedPacketReceiver
    pub fn new(
        inner_receiver: Box<dyn PacketReceiver>,
        link_conditioner_config: &LinkConditionerConfig,
    ) -> Self {
        ConditionedPacketReceiver {
            inner_receiver,
            link_conditioner_config: link_conditioner_config.clone(),
            time_queue: TimeQueue::new(),
            last_payload: None,
        }
    }
}

impl PacketReceiver for ConditionedPacketReceiver {
    fn receive(&mut self) -> Result<Option<(SocketAddr, &[u8])>, RecvError> {
        loop {
            match self.inner_receiver.receive() {
                Ok(option) => match option {
                    None => {
                        break;
                    }
                    Some((addr, buffer)) => {
                        link_condition_logic::process_packet(
                            &self.link_conditioner_config,
                            &mut self.time_queue,
                            (addr, buffer.into()),
                        );
                    }
                },
                Err(err) => {
                    return Err(err);
                }
            }
        }

        if self.time_queue.has_item() {
            let (address, payload) = self.time_queue.pop_item().unwrap();
            self.last_payload = Some(payload);
            return Ok(Some((address, self.last_payload.as_ref().unwrap())));
        } else {
            Ok(None)
        }
    }
}
