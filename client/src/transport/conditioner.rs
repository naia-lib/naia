use naia_shared::{link_condition_logic, Instant, LinkConditionerConfig, TimeQueue};

use crate::transport::udp::UdpPacketReceiver;
use super::{server_addr::ServerAddr, PacketReceiver, RecvError};

/// Used to receive packets from the Client Socket
#[derive(Clone)]
pub struct ConditionedPacketReceiver {
    inner_receiver: UdpPacketReceiver,
    link_conditioner_config: LinkConditionerConfig,
    time_queue: TimeQueue<Box<[u8]>>,
    last_payload: Option<Box<[u8]>>,
}

impl ConditionedPacketReceiver {
    /// Creates a new ConditionedPacketReceiver
    pub fn new(
        inner_receiver: UdpPacketReceiver,
        link_conditioner_config: &LinkConditionerConfig,
    ) -> Self {
        Self {
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

        let now = Instant::now();
        if self.time_queue.has_item(&now) {
            self.last_payload = Some(self.time_queue.pop_item(&now).unwrap());
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
