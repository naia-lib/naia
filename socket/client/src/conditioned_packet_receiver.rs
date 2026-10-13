use naia_socket_shared::{
    link_condition_logic, Instant, LinkConditionerConfig, PollDelayQueue, TimeQueue,
};

use super::{error::NaiaClientSocketError, server_addr::ServerAddr};
use crate::backends::PlainPacketReceiver;

/// Used to receive packets from the Client Socket
#[derive(Clone)]
pub struct ConditionedPacketReceiver {
    inner_receiver: PlainPacketReceiver,
    link_conditioner_config: LinkConditionerConfig,
    time_queue: TimeQueue<Box<[u8]>>,
    poll_queue: PollDelayQueue<Box<[u8]>>,
    poll: u64,
    last_payload: Option<Box<[u8]>>,
}

impl ConditionedPacketReceiver {
    /// Creates a new `ConditionedPacketReceiver`
    #[must_use]
    pub fn new(
        inner_receiver: PlainPacketReceiver,
        link_conditioner_config: &LinkConditionerConfig,
    ) -> Self {
        ConditionedPacketReceiver {
            inner_receiver,
            link_conditioner_config: link_conditioner_config.clone(),
            time_queue: TimeQueue::new(),
            poll_queue: PollDelayQueue::new(),
            poll: 0,
            last_payload: None,
        }
    }

    /// Receives a packet from the Client Socket
    pub fn receive(&mut self) -> Result<Option<&[u8]>, NaiaClientSocketError> {
        self.poll = self
            .poll
            .checked_add(1)
            .expect("ConditionedPacketReceiver poll count exhausted");
        if let Some(delay) = self.link_conditioner_config.fixed_poll_delay {
            debug_assert!(
                self.link_conditioner_config.incoming_jitter == 0
                    && self.link_conditioner_config.incoming_loss == 0.0,
                "fixed_poll_delay mode bypasses jitter/loss draws: leave both at zero",
            );
            let due = self
                .poll
                .checked_add(u64::from(delay))
                .expect("ConditionedPacketReceiver poll count exhausted");
            loop {
                match self.inner_receiver.receive() {
                    Ok(option) => match option {
                        None => {
                            break;
                        }
                        Some(payload) => {
                            self.poll_queue.push(due, payload.into());
                        }
                    },
                    Err(err) => {
                        return Err(err);
                    }
                }
            }
            if self.poll_queue.has_due(self.poll) {
                self.last_payload = Some(self.poll_queue.pop_due(self.poll).unwrap());
                return Ok(Some(self.last_payload.as_ref().unwrap()));
            }
            return Ok(None);
        }
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
            Ok(Some(self.last_payload.as_ref().unwrap()))
        } else {
            Ok(None)
        }
    }

    /// Get the Server's Socket address
    #[must_use]
    pub fn server_addr(&self) -> ServerAddr {
        self.inner_receiver.server_addr()
    }
}
