// Channel Trait
pub trait Channel: 'static {}

// ChannelSettings
#[derive(Clone)]
pub struct ChannelSettings {
    pub mode: ChannelMode,
    pub direction: ChannelDirection,
}

impl ChannelSettings {
    pub fn new(mode: ChannelMode, direction: ChannelDirection) -> Self {
        if mode.tick_buffered() && direction != ChannelDirection::ClientToServer {
            panic!("TickBuffered Messages are only allowed to be sent from Client to Server");
        }

        Self { mode, direction }
    }

    pub fn reliable(&self) -> bool {
        match &self.mode {
            ChannelMode::UnorderedUnreliable => false,
            ChannelMode::SequencedUnreliable => false,
            ChannelMode::UnorderedReliable(_) => true,
            ChannelMode::SequencedReliable(_) => true,
            ChannelMode::OrderedReliable(_) => true,
            ChannelMode::TickBuffered => false,
        }
    }

    pub fn tick_buffered(&self) -> bool {
        self.mode.tick_buffered()
    }

    pub fn can_send_to_server(&self) -> bool {
        match &self.direction {
            ChannelDirection::ClientToServer => true,
            ChannelDirection::ServerToClient => false,
            ChannelDirection::Bidirectional => true,
        }
    }

    pub fn can_send_to_client(&self) -> bool {
        match &self.direction {
            ChannelDirection::ClientToServer => false,
            ChannelDirection::ServerToClient => true,
            ChannelDirection::Bidirectional => true,
        }
    }
}

#[derive(Clone)]
pub struct ReliableSettings {
    pub rtt_resend_factor: f32,
}

impl ReliableSettings {
    pub const fn default() -> Self {
        Self {
            rtt_resend_factor: 1.5,
        }
    }
}

// ChannelMode
#[derive(Clone)]
pub enum ChannelMode {
    UnorderedUnreliable,
    SequencedUnreliable,
    UnorderedReliable(ReliableSettings),
    SequencedReliable(ReliableSettings),
    OrderedReliable(ReliableSettings),
    TickBuffered,
}

impl ChannelMode {
    pub fn tick_buffered(&self) -> bool {
        matches!(self, ChannelMode::TickBuffered)
    }
}

// ChannelDirection
#[derive(Clone, Eq, PartialEq)]
pub enum ChannelDirection {
    ClientToServer,
    ServerToClient,
    Bidirectional,
}
