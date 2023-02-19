use naia_serde::{BitReader, BitWrite, Serde, SerdeErr, UnsignedInteger};
use naia_socket_shared::Instant;

pub const GAME_TIME_LIMIT: u32 = 4194304; // 2^22
const GAME_TIME_LIMIT_U128: u128 = GAME_TIME_LIMIT as u128;
const GAME_TIME_MAX: u32 = 4194303; // 2^22 - 1
const TIME_OFFSET_MAX: i32 = 2097151; // 2^21 - 1
const TIME_OFFSET_MIN: i32 = -2097152; // 2^21 * -1

// GameInstant measures the # of milliseconds since the start of the Server
// GameInstant wraps around at 2^22 milliseconds (around one hour)
#[derive(PartialEq, Clone)]
pub struct GameInstant {
    millis: u32,
}

impl GameInstant {
    pub fn new(start_instant: &Instant) -> Self {
        let millis = (start_instant.elapsed().as_millis() % GAME_TIME_LIMIT_U128) as u32;

        // start_instant should mark the initialization of the Server's TimeManager
        Self { millis }
    }

    // This method assumes that `previous_instant` is known to be from the past
    pub fn time_since(&self, previous_instant: &GameInstant) -> GameDuration {
        let previous_millis = previous_instant.millis;
        let current_millis = self.millis;

        if previous_millis == current_millis {
            return GameDuration { millis: 0 };
        }

        if previous_millis < current_millis {
            return GameDuration::from_millis(current_millis - previous_millis);
        } else {
            return GameDuration::from_millis(GAME_TIME_MAX - previous_millis + current_millis);
        }
    }

    // Returns offset to target time, in milliseconds (possibly negative)
    // 10.offset_from(12) = 2
    // 12.offset_from(10) = -2
    pub fn offset_from(&self, other: &GameInstant) -> i32 {
        const MAX: i32 = TIME_OFFSET_MAX;
        const MIN: i32 = TIME_OFFSET_MIN;
        const ADJUST: i32 = GAME_TIME_LIMIT as i32;

        let a: i32 = self.millis as i32;
        let b: i32 = other.millis as i32;

        let mut result = b - a;
        if (MIN..=MAX).contains(&result) {
            result
        } else if b > a {
            result = b - (a + ADJUST);
            if (MIN..=MAX).contains(&result) {
                result
            } else {
                panic!("integer overflow, this shouldn't happen");
            }
        } else {
            result = (b + ADJUST) - a;
            if (MIN..=MAX).contains(&result) {
                result
            } else {
                panic!("integer overflow, this shouldn't happen");
            }
        }
    }

    pub fn as_millis(&self) -> u32 {
        self.millis
    }

    pub fn add_millis(&self, millis: u32) -> Self {
        Self {
            millis: (self.millis + millis) % GAME_TIME_LIMIT,
        }
    }

    pub fn sub_millis(&self, millis: u32) -> Self {
        if self.millis >= millis {
            Self {
                millis: self.millis - millis,
            }
        } else {
            // my millis is less than your millis
            let delta = millis - self.millis;
            Self {
                millis: GAME_TIME_LIMIT - delta,
            }
        }
    }
}

impl Serde for GameInstant {
    fn ser(&self, writer: &mut dyn BitWrite) {
        let integer = UnsignedInteger::<22>::new(self.millis as u64);
        integer.ser(writer);
    }

    fn de(reader: &mut BitReader) -> Result<Self, SerdeErr> {
        let integer = UnsignedInteger::<22>::de(reader)?;
        let millis = integer.get() as u32;
        Ok(Self { millis })
    }
}

// GameDuration measures the duration between two GameInstants, in milliseconds
#[derive(PartialEq, PartialOrd, Eq, Clone)]
pub struct GameDuration {
    millis: u32,
}

impl GameDuration {
    pub fn from_millis(millis: u32) -> Self {
        Self { millis }
    }

    pub fn as_millis(&self) -> u32 {
        return self.millis;
    }

    pub fn add_millis(&self, millis: u32) -> Self {
        Self { millis: self.millis + millis }
    }
}

// Tests
#[cfg(test)]
mod wrapping_diff_tests {
    use super::GameInstant;
    use crate::game_time::{GAME_TIME_LIMIT, GAME_TIME_MAX};

    #[test]
    fn simple() {
        let a = GameInstant { millis: 10 };
        let b = GameInstant { millis: 12 };

        let result = a.offset_from(&b);

        assert_eq!(result, 2);
    }

    #[test]
    fn simple_backwards() {
        let a = GameInstant { millis: 10 };
        let b = GameInstant { millis: 12 };

        let result = b.offset_from(&a);

        assert_eq!(result, -2);
    }

    #[test]
    fn max_wrap() {
        let a = GameInstant {
            millis: GAME_TIME_MAX,
        };
        let b = a.add_millis(2);

        let result = a.offset_from(&b);

        assert_eq!(result, 2);
    }

    #[test]
    fn min_wrap() {
        let a = GameInstant { millis: 0 };
        let b = a.sub_millis(2);

        let result = a.offset_from(&b);

        assert_eq!(result, -2);
    }

    #[test]
    fn max_wrap_backwards() {
        let a = GameInstant {
            millis: GAME_TIME_MAX,
        };
        let b = a.add_millis(2);

        let result = b.offset_from(&a);

        assert_eq!(result, -2);
    }

    #[test]
    fn min_wrap_backwards() {
        let a = GameInstant { millis: 0 };
        let b = a.sub_millis(2);

        let result = b.offset_from(&a);

        assert_eq!(result, 2);
    }

    #[test]
    fn medium_min_wrap() {
        let diff = (GAME_TIME_LIMIT / 2);
        let a = GameInstant { millis: 0 };
        let b = a.sub_millis(diff);

        let result = a.offset_from(&b);

        assert_eq!(result as i64, -i64::from(diff));
    }

    #[test]
    fn medium_min_wrap_backwards() {
        let diff = (GAME_TIME_LIMIT / 2) - 1;
        let a = GameInstant { millis: 0 };
        let b = a.sub_millis(diff);

        let result = b.offset_from(&a);

        assert_eq!(result as i64, i64::from(diff));
    }

    #[test]
    fn medium_max_wrap() {
        let diff = (GAME_TIME_LIMIT / 2) - 1;
        let a = GameInstant {
            millis: GAME_TIME_MAX,
        };
        let b = a.add_millis(diff);

        let result = a.offset_from(&b);

        assert_eq!(result as i64, i64::from(diff));
    }

    #[test]
    fn medium_max_wrap_backwards() {
        let diff = (GAME_TIME_LIMIT / 2);
        let a = GameInstant {
            millis: GAME_TIME_MAX,
        };
        let b = a.add_millis(diff);

        let result = b.offset_from(&a);

        assert_eq!(result as i64, -i64::from(diff));
    }
}
