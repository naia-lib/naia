use naia_shared::{link_condition_logic, LinkConditionerConfig, TimeQueue};

use super::{server_addr::ServerAddr, PacketReceiver, RecvError};

/// Used to receive packets from the Client Socket
#[derive(Clone)]
pub struct ConditionedPacketReceiver {
    inner_receiver: Box<dyn PacketReceiver>,
    link_conditioner_config: LinkConditionerConfig,
    time_queue: TimeQueue<Box<[u8]>>,
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
    fn receive(&mut self) -> Result<Option<&[u8]>, RecvError> {
        loop {
            match self.inner_receiver.receive() {
                Ok(option) => match option {
                    None => {
                        break;
                    }
                    Some(payload) => {
                        link_condition_logic::process_packet(
                            &self.link_conditioner_config,
                            &mut self.time_queue,
                            payload.into(),
                        );
                    }
                },
                Err(err) => {
                    return Err(err);
                }
            }
        }

        if self.time_queue.has_item() {
            self.last_payload = Some(self.time_queue.pop_item().unwrap());
            return Ok(Some(self.last_payload.as_ref().unwrap()));
        } else {
            Ok(None)
        }
    }

    /// Get the Server's Socket address
    fn server_addr(&self) -> ServerAddr {
        self.inner_receiver.server_addr()
    }
}
