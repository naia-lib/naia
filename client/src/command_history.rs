use std::collections::VecDeque;

use naia_shared::{sequence_greater_than, Tick};

/// How many ticks of command history [`CommandHistory`] retains by default.
///
/// 1200 ticks is one minute at the default `tick_interval` of 50ms (20Hz).
/// Rollback only ever needs to reach back to the last tick the server
/// confirmed, so a minute is far beyond any survivable round trip -- it is
/// sized to be a backstop against unbounded growth, not a tuning knob.
pub const DEFAULT_MAX_TICKS: u16 = 1200;

/// Ring buffer of (tick, command) pairs for client-prediction rollback; old entries are pruned when the server acknowledges a tick.
///
/// Pruning normally happens in [`Self::replays`], driven by server
/// acknowledgements. That leaves the buffer at the mercy of the connection: if
/// acknowledgements stall -- a hitching server, a client that has stopped
/// receiving, a long stretch of packet loss -- nothing prunes, and the buffer
/// grows for as long as the client keeps predicting. So insertion also
/// enforces a hard ceiling of `max_ticks` worth of history behind the newest
/// command, which bounds memory no matter what the network does.
pub struct CommandHistory<T: Clone> {
    buffer: VecDeque<(Tick, T)>,
    max_ticks: u16,
}

impl<T: Clone> Default for CommandHistory<T> {
    /// Retains [`DEFAULT_MAX_TICKS`] ticks of history.
    fn default() -> Self {
        Self::new(DEFAULT_MAX_TICKS)
    }
}

impl<T: Clone> CommandHistory<T> {
    /// Creates a history retaining at most `max_ticks` of history behind the
    /// most recent command. Prefer [`Default`] unless you have measured a
    /// reason to differ.
    ///
    /// The bound is a span of ticks, not a count of entries: commands need not
    /// be inserted on every tick, and what matters for rollback is how far
    /// back in time the buffer reaches. Because [`Self::insert`] requires
    /// strictly increasing ticks, each retained entry occupies a distinct
    /// tick, so bounding the span bounds the entry count too (at `max_ticks + 1`).
    pub fn new(max_ticks: u16) -> Self {
        Self {
            buffer: VecDeque::new(),
            max_ticks,
        }
    }

    /// Drops all history up to and including `start_tick`, then returns all remaining (tick, command) pairs for replay.
    pub fn replays(&mut self, start_tick: &Tick) -> Vec<(Tick, T)> {
        // Remove history of commands until current received tick
        self.remove_to_and_including(*start_tick);

        // Get copies of all remaining stored Commands
        let mut output = Vec::new();

        for (tick, command) in self.buffer.iter() {
            output.push((*tick, command.clone()));
        }

        output
    }

    /// Appends `new_command` at `command_tick`; panics if `command_tick` is not strictly later than the last inserted tick.
    // this only goes forward
    pub fn insert(&mut self, command_tick: Tick, new_command: T) {
        if let Some((last_most_recent_command_tick, _)) = self.buffer.back() {
            if !sequence_greater_than(command_tick, *last_most_recent_command_tick) {
                panic!("You must always insert a more recent command into the CommandHistory than the one you last inserted.");
            }
        }

        // go ahead and push
        self.buffer.push_back((command_tick, new_command));

        self.evict_beyond_max_ticks(command_tick);
    }

    /// Drops entries more than `max_ticks` behind `newest_tick`.
    ///
    /// Distance is computed with `wrapping_sub` so this stays correct across
    /// the `u16` tick wrap. `insert` guarantees ticks arrive in increasing
    /// order, so the front is always the oldest and eviction can stop at the
    /// first entry still inside the window.
    fn evict_beyond_max_ticks(&mut self, newest_tick: Tick) {
        while let Some((oldest_tick, _)) = self.buffer.front() {
            if newest_tick.wrapping_sub(*oldest_tick) <= self.max_ticks {
                return;
            }
            self.buffer.pop_front();
        }
    }

    fn remove_to_and_including(&mut self, index: Tick) {
        loop {
            let back_index = match self.buffer.front() {
                Some((index, _)) => *index,
                None => {
                    return;
                }
            };
            if sequence_greater_than(back_index, index) {
                return;
            }
            self.buffer.pop_front();
        }
    }

    /// Returns `true` if `tick` is strictly later than the most-recently inserted tick, meaning a new command can be appended.
    pub fn can_insert(&self, tick: &Tick) -> bool {
        if let Some((last_most_recent_command_tick, _)) = self.buffer.back() {
            if !sequence_greater_than(*tick, *last_most_recent_command_tick) {
                return false;
            }
        }
        true
    }

    /// Returns the tick of the most-recently buffered command, or `None` if the buffer is empty.
    pub fn most_recent_tick(&self) -> Option<Tick> {
        self.buffer.back().map(|(tick, _)| *tick)
    }

    /// Non-consuming lookup of the buffered command at `tick`, or `None` if it
    /// isn't present (never buffered, or already pruned by [`Self::replays`]).
    ///
    /// Unlike `replays`, this does not mutate the buffer — so a reader that
    /// trails the prediction front (the client-confirmed re-simulation) can
    /// re-derive a past tick's command without disturbing the rollback-replay
    /// buffer. Callers must read BEFORE the rollback prunes the tick (the
    /// confirmed re-sim runs in `HandleTickEvents`, before the `Rollback` set).
    pub fn get(&self, tick: &Tick) -> Option<&T> {
        self.buffer
            .iter()
            .find(|(t, _)| t == tick)
            .map(|(_, command)| command)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fills `history` with one command per tick starting at `start`.
    fn insert_range(history: &mut CommandHistory<u16>, start: Tick, count: u16) {
        for i in 0..count {
            let tick = start.wrapping_add(i);
            history.insert(tick, tick);
        }
    }

    fn span(history: &CommandHistory<u16>) -> u16 {
        let front = history.buffer.front().unwrap().0;
        let back = history.buffer.back().unwrap().0;
        back.wrapping_sub(front)
    }

    /// The bug this bound exists for: `replays` is the only other pruner, and
    /// it only runs when the server acknowledges a tick. A client that keeps
    /// predicting while acknowledgements stall used to grow without limit.
    #[test]
    fn insert_alone_cannot_grow_without_bound() {
        let mut history = CommandHistory::default();
        insert_range(&mut history, 0, 10_000);

        assert!(history.buffer.len() <= DEFAULT_MAX_TICKS as usize + 1);
        assert_eq!(span(&history), DEFAULT_MAX_TICKS);
    }

    #[test]
    fn eviction_drops_the_oldest_and_keeps_the_newest() {
        let mut history = CommandHistory::new(10);
        insert_range(&mut history, 0, 25);

        // Newest is retained, everything outside the window is gone.
        assert_eq!(history.most_recent_tick(), Some(24));
        assert_eq!(history.get(&24), Some(&24));
        assert_eq!(history.get(&14), Some(&14));
        assert_eq!(history.get(&13), None);
        assert_eq!(history.get(&0), None);
    }

    /// The bound is a span, not an entry count: sparse commands must still
    /// reach the full `max_ticks` back rather than being evicted early.
    #[test]
    fn bound_is_a_tick_span_not_an_entry_count() {
        let mut history = CommandHistory::new(100);
        // One command every 10 ticks -- only 11 entries cover the window.
        for i in 0..20u16 {
            history.insert(i * 10, i);
        }

        assert_eq!(history.most_recent_tick(), Some(190));
        assert_eq!(history.get(&90), Some(&9)); // exactly 100 ticks back
        assert_eq!(history.get(&80), None); // 110 ticks back, evicted
        assert_eq!(history.buffer.len(), 11);
    }

    #[test]
    fn eviction_is_correct_across_the_tick_wrap() {
        let mut history = CommandHistory::new(10);
        // Straddle the u16 boundary: 65530..=65535 then 0..=4.
        insert_range(&mut history, u16::MAX - 5, 11);

        assert_eq!(history.most_recent_tick(), Some(4));
        assert_eq!(span(&history), 10);
        assert_eq!(history.get(&u16::MAX), Some(&u16::MAX));

        // One more tick pushes the oldest out of the window.
        history.insert(5, 5);
        assert_eq!(history.get(&(u16::MAX - 5)), None);
        assert_eq!(span(&history), 10);
    }

    #[test]
    fn replays_still_prunes_acknowledged_ticks() {
        let mut history = CommandHistory::new(100);
        insert_range(&mut history, 0, 10);

        let replayed = history.replays(&4);

        assert_eq!(replayed, vec![(5, 5), (6, 6), (7, 7), (8, 8), (9, 9)]);
        assert_eq!(history.get(&4), None);
    }

    #[test]
    fn a_zero_bound_retains_only_the_newest_command() {
        let mut history = CommandHistory::new(0);
        insert_range(&mut history, 0, 5);

        assert_eq!(history.buffer.len(), 1);
        assert_eq!(history.most_recent_tick(), Some(4));
    }

    #[test]
    fn can_insert_and_insert_agree_after_eviction() {
        let mut history = CommandHistory::new(10);
        insert_range(&mut history, 0, 25);

        // Eviction must not disturb the monotonic-insert contract.
        assert!(!history.can_insert(&24));
        assert!(!history.can_insert(&5));
        assert!(history.can_insert(&25));
    }
}
