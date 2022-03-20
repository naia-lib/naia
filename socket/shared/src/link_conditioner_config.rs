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
            incoming_latency: 50,
            incoming_jitter: 10,
            incoming_loss: 0.01,
        }
    }

    /// Creates a new LinkConditioner that simulates a connection which is in an
    /// average condition
    pub fn average_condition() -> Self {
        LinkConditionerConfig {
            incoming_latency: 200,
            incoming_jitter: 20,
            incoming_loss: 0.055,
        }
    }

    /// Creates a new LinkConditioner that simulates a connection which is in an
    /// poor condition
    pub fn poor_condition() -> Self {
        LinkConditionerConfig {
            incoming_latency: 350,
            incoming_jitter: 30,
            incoming_loss: 0.1,
        }
    }
}
