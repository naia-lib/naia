use std::{default::Default, time::Duration};

use naia_shared::{ConnectionConfig, DEFAULT_CAUSE_EVIDENCE_WINDOW};

use crate::JitterBufferType;

/// Contains Config properties which will be used by a Server or Client
#[derive(Clone)]
pub struct ClientConfig {
    /// Used to configure the connection with the Server
    pub connection: ConnectionConfig,
    /// The duration between the resend of certain connection handshake messages
    pub send_handshake_interval: Duration,
    /// The duration to wait before sending a ping message to the remote host,
    /// in order to estimate RTT time
    pub ping_interval: Duration,
    /// The number of network samples to take before completing the Connection Handshake.
    /// Increase this for greater accuracy of network statistics, at the cost of the handshake
    /// taking longer. Keep in mind that the network measurements affect how likely commands
    /// are able to arrive at the server before processing.
    pub handshake_pings: u8,
    /// Configuration for jitter buffer behavior
    pub jitter_buffer: JitterBufferType,
    /// nydus-dm9 evidence opt-in (Ada 184483): record applied-cause evidence
    /// for the causal-proof matcher. Default off: no push, no mark and no
    /// allocation — the journal stays dark.
    pub cause_evidence: bool,
    /// Retention window in ticks behind the applied frontier while
    /// `cause_evidence` is on. Older history is evicted, each eviction
    /// declared as one refusal lapse. Default: 16 (the Diax matcher window).
    pub cause_evidence_window: u16,
    /// Default deadline for outgoing requests whose type does not override
    /// [`Request::timeout`](naia_shared::Request::timeout) (messaging-24
    /// t3). Applies from send; a request still unanswered past it is
    /// cancelled locally and polls
    /// [`TimedOut`](naia_shared::RequestPoll::TimedOut).
    pub request_timeout: Duration,
}

impl Default for ClientConfig {
    fn default() -> Self {
        // Under `test_time`, default to `Bypass`: the deterministic `TestClock` makes
        // production-style jitter smoothing meaningless (and racy with delivery), so
        // the client should follow delivery directly. The receiving-tick cap in
        // `time_manager::collect_ticks` enforces "don't reconstruct an unreceived
        // tick" universally — together with `Bypass` this gives a fully deterministic
        // confirmed timeline in tests. Production builds keep `Real` + RTT margins.
        #[cfg(feature = "test_time")]
        let jitter_buffer = JitterBufferType::Bypass;
        #[cfg(not(feature = "test_time"))]
        let jitter_buffer = JitterBufferType::Real;

        Self {
            connection: ConnectionConfig::default(),
            send_handshake_interval: Duration::from_millis(250),
            ping_interval: Duration::from_secs(1),
            handshake_pings: 10,
            jitter_buffer,
            cause_evidence: false,
            cause_evidence_window: DEFAULT_CAUSE_EVIDENCE_WINDOW,
            request_timeout: Duration::from_secs(30),
        }
    }
}
