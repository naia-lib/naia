use std::collections::VecDeque;

use crate::{sequence_greater_than, GameInstant};

pub type PingIndex = u16;

const SENT_PINGS_HISTORY_SIZE: u16 = 32;

pub struct PingStore {
    ping_index: PingIndex,
    // front big, back small
    // front recent, back past
    buffer: VecDeque<(PingIndex, GameInstant)>,
}

impl PingStore {
    pub fn new() -> Self {
        PingStore {
            ping_index: 0,
            buffer: VecDeque::new(),
        }
    }

    pub fn push_new(&mut self, now: GameInstant) -> PingIndex {
        // save current ping index and add a new ping instant associated with it
        let ping_index = self.ping_index;
        self.ping_index = self.ping_index.wrapping_add(1);
        self.buffer.push_front((ping_index, now));

        // a good time to prune down the size of this buffer
        while self.buffer.len() > SENT_PINGS_HISTORY_SIZE.into() {
            self.buffer.pop_back();
        }

        ping_index
    }

    pub fn remove(&mut self, ping_index: PingIndex) -> Option<GameInstant> {
        let mut vec_index = self.buffer.len();

        if vec_index == 0 {
            return None;
        }

        let mut found = false;

        loop {
            vec_index -= 1;

            if let Some((old_index, _)) = self.buffer.get(vec_index) {
                if *old_index == ping_index {
                    //found it!
                    found = true;
                } else {
                    // if old_index is bigger than ping_index, give up, it's only getting
                    // bigger
                    if sequence_greater_than(*old_index, ping_index) {
                        return None;
                    }
                }
            }

            if found {
                let (_, ping_instant) = self.buffer.remove(vec_index).unwrap();
                return Some(ping_instant);
            }

            // made it to the front
            if vec_index == 0 {
                return None;
            }
        }
    }

    pub fn clear(&mut self) {
        self.buffer.clear();
    }
}
