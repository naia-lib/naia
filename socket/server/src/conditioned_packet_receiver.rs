use std::{net::SocketAddr, sync::Arc};

use smol::channel::Receiver;

use naia_socket_shared::{
    link_condition_logic, Instant, LinkConditionerConfig, PollDelayQueue, TimeQueue,
};

use super::{error::NaiaServerSocketError, shutdown::ShutdownSignal};

/// Used to receive packets from the Server Socket
#[derive(Clone)]
pub struct ConditionedPacketReceiver {
    #[allow(clippy::type_complexity)]
    channel_receiver: Receiver<Result<(SocketAddr, Box<[u8]>), NaiaServerSocketError>>,
    link_conditioner_config: LinkConditionerConfig,
    time_queue: TimeQueue<(SocketAddr, Box<[u8]>)>,
    poll_queue: PollDelayQueue<(SocketAddr, Box<[u8]>)>,
    poll: u64,
    last_payload: Option<Box<[u8]>>,
    // Shared shutdown trigger for the listen that created this handle: the
    // last handle drop ends the background tasks (naia-lib/naia#92).
    // Retained, never read. Set via PacketReceiver::with_shutdown.
    #[allow(dead_code)]
    pub(crate) shutdown: Option<Arc<ShutdownSignal>>,
}

impl ConditionedPacketReceiver {
    /// Creates a new `ConditionedPacketReceiver`
    #[allow(clippy::type_complexity)]
    #[must_use]
    pub fn new(
        channel_receiver: Receiver<Result<(SocketAddr, Box<[u8]>), NaiaServerSocketError>>,
        link_conditioner_config: &LinkConditionerConfig,
    ) -> Self {
        Self {
            channel_receiver,
            link_conditioner_config: link_conditioner_config.clone(),
            time_queue: TimeQueue::new(),
            poll_queue: PollDelayQueue::new(),
            poll: 0,
            last_payload: None,
            shutdown: None,
        }
    }

    /// Receives a packet from the Server Socket
    ///
    /// # Panics
    ///
    /// Panics if the time queue reports an item but yields none, or the
    /// stored payload slot is empty after storing a payload (both
    /// unreachable: the slot is filled immediately above).
    // `Err` is currently unreachable, but callers branch on it so a
    // persistently-failing channel cannot trap a drain loop; keep the
    // signature fallible.
    #[allow(clippy::unnecessary_wraps)]
    pub fn receive(&mut self) -> Result<Option<(SocketAddr, &[u8])>, NaiaServerSocketError> {
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
            while let Ok(result) = self.channel_receiver.try_recv() {
                match result {
                    Ok(packet) => {
                        self.poll_queue.push(due, packet);
                    }
                    Err(_) => {
                        break; //TODO: Handle error here
                    }
                }
            }
            if self.poll_queue.has_due(self.poll) {
                let (address, payload) = self.poll_queue.pop_due(self.poll).unwrap();
                self.last_payload = Some(payload);
                return Ok(Some((address, self.last_payload.as_ref().unwrap())));
            }
            return Ok(None);
        }
        while let Ok(result) = self.channel_receiver.try_recv() {
            match result {
                Ok(packet) => {
                    link_condition_logic::process_packet(
                        &self.link_conditioner_config,
                        &mut self.time_queue,
                        packet,
                    );
                }
                Err(_) => {
                    break; //TODO: Handle error here
                }
            }
        }

        let now = Instant::now();
        if self.time_queue.has_item(&now) {
            let (address, payload) = self.time_queue.pop_item(&now).unwrap();
            self.last_payload = Some(payload);
            Ok(Some((address, self.last_payload.as_ref().unwrap())))
        } else {
            Ok(None)
        }
    }
}
