/// Contains configuration required to initialize a `LinkConditioner`
#[derive(Clone)]
pub struct LinkConditionerConfig {
    /// Delay to receive incoming messages in milliseconds
    pub incoming_latency: u32,
    /// The maximum additional random latency to delay received incoming
    /// messages in milliseconds. This may be added OR subtracted from the
    /// latency determined in the `incoming_latency` property above
    pub incoming_jitter: u32,
    /// The % chance that an incoming packet will be dropped.
    /// Represented as a value between 0 and 1
    pub incoming_loss: f32,
    /// When set, packets are delayed by exactly this many consumer
    /// [`receive`](crate::link_condition_logic) polls instead of by
    /// wall-clock milliseconds, so arrival-poll maps to delivery-poll
    /// identically on every run. Tick-determinism gates use this; real links
    /// keep the field `None` and condition on wall-clock time. Jitter and
    /// loss still draw from thread randomness when nonzero, so deterministic
    /// runs must leave them at zero; the receivers `debug_assert` that.
    pub fixed_poll_delay: Option<u32>,
}

impl LinkConditionerConfig {
    /// Creates a new `LinkConditionerConfig`
    #[must_use]
    pub fn new(incoming_latency: u32, incoming_jitter: u32, incoming_loss: f32) -> Self {
        Self {
            incoming_latency,
            incoming_jitter,
            incoming_loss,
            fixed_poll_delay: None,
        }
    }

    /// Creates a `LinkConditionerConfig` that delays every packet by exactly
    /// `polls` consumer polls with zero jitter and zero loss, for
    /// tick-deterministic runs. A packet taken in on poll `p` becomes due on
    /// poll `p + polls`, independent of wall-clock speed or thread
    /// scheduling. This exercises late delivery (the property latency gates
    /// care about) without the run-to-run tick shift wall-clock deadlines
    /// introduce when ticks advance faster than real time.
    #[must_use]
    pub fn deterministic(polls: u32) -> Self {
        Self {
            incoming_latency: 0,
            incoming_jitter: 0,
            incoming_loss: 0.0,
            fixed_poll_delay: Some(polls),
        }
    }

    #[must_use]
    pub fn perfect_condition() -> Self {
        Self {
            incoming_latency: 1,
            incoming_jitter: 0,
            incoming_loss: 0.0,
            fixed_poll_delay: None,
        }
    }

    /// Creates a new `LinkConditioner` that simulates a connection which is in a
    /// very good condition
    #[must_use]
    pub fn very_good_condition() -> Self {
        Self {
            incoming_latency: 12,
            incoming_jitter: 3,
            incoming_loss: 0.001,
            fixed_poll_delay: None,
        }
    }

    /// Creates a new `LinkConditioner` that simulates a connection which is in a
    /// good condition
    #[must_use]
    pub fn good_condition() -> Self {
        Self {
            incoming_latency: 40,
            incoming_jitter: 10,
            incoming_loss: 0.002,
            fixed_poll_delay: None,
        }
    }

    /// Creates a new `LinkConditioner` that simulates a connection which is in an
    /// average condition
    #[must_use]
    pub fn average_condition() -> Self {
        Self {
            incoming_latency: 100,
            incoming_jitter: 25,
            incoming_loss: 0.02,
            fixed_poll_delay: None,
        }
    }

    /// Creates a new `LinkConditioner` that simulates a connection which is in an
    /// poor condition
    #[must_use]
    pub fn poor_condition() -> Self {
        Self {
            incoming_latency: 200,
            incoming_jitter: 50,
            incoming_loss: 0.04,
            fixed_poll_delay: None,
        }
    }

    /// Creates a new `LinkConditioner` that simulates a connection which is in an
    /// very poor condition
    #[must_use]
    pub fn very_poor_condition() -> Self {
        Self {
            incoming_latency: 300,
            incoming_jitter: 75,
            incoming_loss: 0.06,
            fixed_poll_delay: None,
        }
    }
}
