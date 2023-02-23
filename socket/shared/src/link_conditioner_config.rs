/// Contains configuration required to initialize a LinkConditioner
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
}

impl LinkConditionerConfig {
    /// Creates a new LinkConditionerConfig
    pub fn new(incoming_latency: u32, incoming_jitter: u32, incoming_loss: f32) -> Self {
        LinkConditionerConfig {
            incoming_latency,
            incoming_jitter,
            incoming_loss,
        }
    }

    /// Creates a new LinkConditioner that simulates a connection which is in a
    /// good condition
    pub fn good_condition() -> Self {
        LinkConditionerConfig {
            incoming_latency: 40,
            incoming_jitter: 6,
            incoming_loss: 0.002,
        }
    }

    /// Creates a new LinkConditioner that simulates a connection which is in an
    /// average condition
    pub fn average_condition() -> Self {
        LinkConditionerConfig {
            incoming_latency: 170,
            incoming_jitter: 45,
            incoming_loss: 0.02,
        }
    }

    /// Creates a new LinkConditioner that simulates a connection which is in an
    /// poor condition
    pub fn poor_condition() -> Self {
        LinkConditionerConfig {
            incoming_latency: 300,
            incoming_jitter: 84,
            incoming_loss: 0.04,
        }
    }
}
