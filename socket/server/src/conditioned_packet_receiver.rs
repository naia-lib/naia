use std::{net::SocketAddr, sync::Arc};

use smol::channel::Receiver;

use naia_socket_shared::{
    link_condition_logic, Instant, LinkConditionerConfig, PollDelayQueue, TimeQueue,
};

use super::{error::NaiaServerSocketError, shutdown::ShutdownSignal};

/// Used to receive packets from the Server Socket
#[derive(Clone)]
pub struct ConditionedPacketReceiver {
    #[expect(
        clippy::type_complexity,
        reason = "the channel carries an address, payload and transport error as one typed event"
    )]
    channel_receiver: Receiver<Result<(SocketAddr, Box<[u8]>), NaiaServerSocketError>>,
    link_conditioner_config: LinkConditionerConfig,
    time_queue: TimeQueue<(SocketAddr, Box<[u8]>)>,
    poll_queue: PollDelayQueue<(SocketAddr, Box<[u8]>)>,
    poll: u64,
    last_payload: Option<Box<[u8]>>,
    // Shared shutdown trigger for the listen that created this handle: the
    // last handle drop ends the background tasks (naia-lib/naia#92).
    // Retained, never read. Set via PacketReceiver::with_shutdown.
    pub(crate) shutdown: Option<Arc<ShutdownSignal>>,
}

impl ConditionedPacketReceiver {
    /// Creates a new `ConditionedPacketReceiver`
    #[expect(
        clippy::type_complexity,
        reason = "the channel carries an address, payload and transport error as one typed event"
    )]
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
    /// Panics once the poll counter exhausts `u64`.
    // `Err` is currently unreachable, but callers branch on it so a
    // persistently-failing channel cannot trap a drain loop; keep the
    // signature fallible.
    #[expect(
        clippy::expect_used,
        reason = "a u64 poll counter cannot exhaust; a wrapped counter would misorder delayed packets"
    )]
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
            if let Some((address, payload)) = self.poll_queue.pop_due(self.poll) {
                return Ok(Some((address, self.last_payload.insert(payload))));
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
        match self.time_queue.pop_item(&now) {
            Some((address, payload)) => Ok(Some((address, self.last_payload.insert(payload)))),
            None => Ok(None),
        }
    }
}
