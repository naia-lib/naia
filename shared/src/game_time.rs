use naia_serde::{BitReader, BitWrite, ConstBitLength, Serde, SerdeErr, UnsignedInteger};
use naia_socket_shared::Instant;

const GAME_INSANT_BITS: u8 = 22;
/// Wrapping period of [`GameInstant`] in milliseconds (2^22 ≈ 70 minutes).
pub const GAME_TIME_LIMIT: u32 = 4194304; // 2^22
const GAME_TIME_LIMIT_U128: u128 = 4194304;
const GAME_TIME_MAX: u32 = 4194303; // 2^22 - 1
const TIME_OFFSET_MAX: i32 = 2097151; // 2^21 - 1
const TIME_OFFSET_MIN: i32 = -2097152; // 2^21 * -1

/// Server-relative millisecond timestamp that wraps at 2^22 ms (~70 minutes).
#[derive(PartialEq, Debug, Clone, Copy)]
pub struct GameInstant {
    millis: u32,
}

impl GameInstant {
    /// Creates a `GameInstant` representing the current time relative to `start_instant`.
    pub fn new(start_instant: &Instant) -> Self {
        let now = Instant::now();
        let millis = (start_instant.elapsed(&now).as_millis() % GAME_TIME_LIMIT_U128) as u32;

        // start_instant should mark the initialization of the Server's TimeManager
        Self { millis }
    }

    /// Returns the duration elapsed since `previous_instant` (assumed to be in the past).
    pub fn time_since(&self, previous_instant: &GameInstant) -> GameDuration {
        let previous_millis = previous_instant.millis;
        let current_millis = self.millis;

        if previous_millis == current_millis {
            return GameDuration { millis: 0 };
        }

        if previous_millis < current_millis {
            GameDuration::from_millis(current_millis - previous_millis)
        } else {
            GameDuration::from_millis(GAME_TIME_MAX - previous_millis + current_millis)
        }
    }

    /// Signed millisecond offset to `other` (positive = `other` is later). Wraps correctly at 2^22.
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

    /// Returns `true` if `self` is strictly later than `other` (wrapping-aware).
    /// Returns `true` if `self` is strictly later than `other` (wrapping-aware).
    pub fn is_more_than(&self, other: &GameInstant) -> bool {
        self.offset_from(other) < 0
    }

    /// Returns the raw millisecond value (in `[0, GAME_TIME_LIMIT)`).
    pub fn as_millis(&self) -> u32 {
        self.millis
    }

    /// Returns a new `GameInstant` `millis` milliseconds in the future (wrapping).
    pub fn add_millis(&self, millis: u32) -> Self {
        Self {
            millis: (self.millis + millis) % GAME_TIME_LIMIT,
        }
    }

    /// Returns a new `GameInstant` `millis` milliseconds in the past (wrapping).
    pub fn sub_millis(&self, millis: u32) -> Self {
        let millis = millis % GAME_TIME_LIMIT;
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

    /// Returns a new `GameInstant` offset by `millis` (positive = future, negative = past).
    pub fn add_signed_millis(&self, millis: i32) -> Self {
        if millis >= 0 {
            self.add_millis(millis as u32)
        } else {
            self.sub_millis(-millis as u32)
        }
    }
}

impl Serde for GameInstant {
    fn ser(&self, writer: &mut dyn BitWrite) {
        let integer = UnsignedInteger::<GAME_INSANT_BITS>::new(self.millis as u64);
        integer.ser(writer);
    }

    fn de(reader: &mut BitReader) -> Result<Self, SerdeErr> {
        let integer = UnsignedInteger::<GAME_INSANT_BITS>::de(reader)?;
        let millis = integer.get() as u32;
        Ok(Self { millis })
    }

    fn bit_length(&self) -> u32 {
        <Self as ConstBitLength>::const_bit_length()
    }
}

impl ConstBitLength for GameInstant {
    fn const_bit_length() -> u32 {
        <UnsignedInteger<GAME_INSANT_BITS> as ConstBitLength>::const_bit_length()
    }
}

/// Unsigned millisecond duration between two [`GameInstant`] values.
#[derive(PartialEq, PartialOrd, Eq, Clone)]
pub struct GameDuration {
    millis: u32,
}

impl GameDuration {
    /// Creates a `GameDuration` of `millis` milliseconds.
    pub fn from_millis(millis: u32) -> Self {
        Self { millis }
    }

    /// Returns the duration in milliseconds.
    pub fn as_millis(&self) -> u32 {
        self.millis
    }

    /// Returns a new duration extended by `millis` milliseconds.
    pub fn add_millis(&self, millis: u32) -> Self {
        Self {
            millis: self.millis + millis,
        }
    }

    /// Returns a new duration reduced by `millis` milliseconds.
    pub fn sub_millis(&self, millis: u32) -> Self {
        Self {
            millis: self.millis - millis,
        }
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
        let diff = GAME_TIME_LIMIT / 2;
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
        let diff = GAME_TIME_LIMIT / 2;
        let a = GameInstant {
            millis: GAME_TIME_MAX,
        };
        let b = a.add_millis(diff);

        let result = b.offset_from(&a);

        assert_eq!(result as i64, -i64::from(diff));
    }
}
