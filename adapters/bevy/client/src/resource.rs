use std::{collections::VecDeque, mem::swap};

use naia_client::{Event, NaiaClientError, ProtocolType};

use naia_bevy_shared::Entity;

pub struct ClientResource<P: ProtocolType> {
    events: VecDeque<Result<Event<P, Entity>, NaiaClientError>>,
}

impl<P: ProtocolType> ClientResource<P> {
    pub fn new() -> Self {
        Self {
            events: VecDeque::new(),
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
