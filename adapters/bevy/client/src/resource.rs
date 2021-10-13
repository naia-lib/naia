use std::{collections::VecDeque, mem::swap};

use naia_client::{Event, NaiaClientError, ProtocolType};

use naia_bevy_shared::Entity;

use super::flag::Flag;

pub struct ClientResource<P: ProtocolType> {
    events: VecDeque<Result<Event<P, Entity>, NaiaClientError>>,
    pub ticker: Flag,
    pub connector: Flag,
    pub disconnector: Flag,
}

impl<P: ProtocolType> ClientResource<P> {
    pub fn new() -> Self {
        Self {
            events: VecDeque::new(),
            ticker: Flag::new(),
            connector: Flag::new(),
            disconnector: Flag::new(),
        }
    }

    // Events //

    pub fn push_event(&mut self, event_result: Result<Event<P, Entity>, NaiaClientError>) {
        self.events.push_back(event_result);
    }

    pub fn take_events(&mut self) -> VecDeque<Result<Event<P, Entity>, NaiaClientError>> {
        let mut output = VecDeque::new();
        swap(&mut self.events, &mut output);
        return output;
    }
}
