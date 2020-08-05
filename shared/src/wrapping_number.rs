use crate::{standard_header::StandardHeader, HostType};
use std::convert::TryFrom;

#[derive(Debug)]
pub struct RemoteTickManager {
    tick_latency: i8,
    last_sent_tick: Option<u16>,
    last_received_tick: u16,
}

impl RemoteTickManager {
    pub fn new() -> Self {
        RemoteTickManager {
            tick_latency: 0,
            last_sent_tick: None,
            last_received_tick: 0,
        }
    }

    pub fn get_tick_latency(&mut self, current_tick: u16) -> i8 {
        match self.last_sent_tick {
            None => {
                self.last_sent_tick = Some(current_tick);
                return self.tick_latency;
            }
            Some(last_tick) => {
                if last_tick == current_tick {
                    return self.tick_latency;
                } else {
                    return std::i8::MAX;
                }
            }
        }
    }

    pub fn process_incoming(
        &mut self,
        host_tick: u16,
        header: &StandardHeader,
        host_type: HostType,
    ) {
        let remote_tick = header.tick();
        let remote_tick_diff = wrapping_diff(self.last_received_tick, remote_tick);

        if remote_tick_diff <= 0 {
            return;
        }
        self.last_received_tick = remote_tick;

        let tick_latency = header.tick_latency();

        let mut tick_diff = wrapping_diff(remote_tick, host_tick);

        let max_tick_diff: i16 = std::i8::MAX.into();
        let min_tick_diff: i16 = (std::i8::MIN + 1).into();

        if tick_diff > max_tick_diff {
            tick_diff = max_tick_diff;
        }
        if tick_diff < min_tick_diff {
            tick_diff = min_tick_diff;
        }
        if let Ok(diff) = i8::try_from(tick_diff) {
            // TODO: need to average these diffs out over time
            self.tick_latency = diff;
            self.last_sent_tick = None;
        }

        if HostType::Server == host_type {
            println!(
                "Received Header. Host Tick: {}, Remote->Host Latency: {}, Remote Tick: {}, Host->Remote Latency: {}",
                host_tick, self.tick_latency, remote_tick, tick_latency
            );
        }
    }
}

/// Retrieves the wrapping difference between 2 u16 values
pub fn wrapping_diff(a: u16, b: u16) -> i16 {
    const MAX: i32 = std::i16::MAX as i32;
    const MIN: i32 = std::i16::MIN as i32;
    const ADJUST: i32 = (std::u16::MAX as i32) + 1;

    let a: i32 = i32::from(a);
    let b: i32 = i32::from(b);

    let mut result = b - a;
    if result <= MAX && result >= MIN {
        return result as i16;
    } else {
        if b > a {
            result = b - (a + ADJUST);
            if result <= MAX && result >= MIN {
                return result as i16;
            } else {
                panic!("integer overflow, this shouldn't happen")
            }
        } else {
            result = (b + ADJUST) - a;
            if result <= MAX && result >= MIN {
                return result as i16;
            } else {
                panic!("integer overflow, this shouldn't happen")
            }
        }
    }
}

#[cfg(test)]
mod wrapping_diff_tests {
    use crate::remote_tick_manager::wrapping_diff;

    #[test]
    fn simple() {
        let a: u16 = 10;
        let b: u16 = 12;

        let result = wrapping_diff(a, b);

        assert_eq!(result, 2);
    }

    #[test]
    fn simple_backwards() {
        let a: u16 = 10;
        let b: u16 = 12;

        let result = wrapping_diff(b, a);

        assert_eq!(result, -2);
    }

    #[test]
    fn max_wrap() {
        let a: u16 = std::u16::MAX;
        let b: u16 = a.wrapping_add(2);

        let result = wrapping_diff(a, b);

        assert_eq!(result, 2);
    }

    #[test]
    fn min_wrap() {
        let a: u16 = 0;
        let b: u16 = a.wrapping_sub(2);

        let result = wrapping_diff(a, b);

        assert_eq!(result, -2);
    }

    #[test]
    fn max_wrap_backwards() {
        let a: u16 = std::u16::MAX;
        let b: u16 = a.wrapping_add(2);

        let result = wrapping_diff(b, a);

        assert_eq!(result, -2);
    }

    #[test]
    fn min_wrap_backwards() {
        let a: u16 = 0;
        let b: u16 = a.wrapping_sub(2);

        let result = wrapping_diff(b, a);

        assert_eq!(result, 2);
    }

    #[test]
    fn medium_min_wrap() {
        let diff: u16 = std::u16::MAX / 2;
        let a: u16 = 0;
        let b: u16 = a.wrapping_sub(diff);

        let result = i32::from(wrapping_diff(a, b));

        assert_eq!(result, i32::from(diff) * -1);
    }

    #[test]
    fn medium_min_wrap_backwards() {
        let diff: u16 = std::u16::MAX / 2;
        let a: u16 = 0;
        let b: u16 = a.wrapping_sub(diff);

        let result = i32::from(wrapping_diff(b, a));

        assert_eq!(result, i32::from(diff));
    }

    #[test]
    fn medium_max_wrap() {
        let diff: u16 = std::u16::MAX / 2;
        let a: u16 = std::u16::MAX;
        let b: u16 = a.wrapping_add(diff);

        let result = i32::from(wrapping_diff(a, b));

        assert_eq!(result, i32::from(diff));
    }

    #[test]
    fn medium_max_wrap_backwards() {
        let diff: u16 = (std::u16::MAX / 2);
        let a: u16 = std::u16::MAX;
        let b: u16 = a.wrapping_add(diff);

        let result = i32::from(wrapping_diff(b, a));

        assert_eq!(result, i32::from(diff) * -1);
    }
}
