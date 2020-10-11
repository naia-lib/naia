/// Returns whether or not a wrapping number is greater than another
pub fn sequence_greater_than(s1: u16, s2: u16) -> bool {
    ((s1 > s2) && (s1 - s2 <= 32768)) || ((s1 < s2) && (s2 - s1 > 32768))
}

/// Returns whether or not a wrapping number is greater than another
pub fn sequence_less_than(s1: u16, s2: u16) -> bool {
    sequence_greater_than(s2, s1)
}

/// Retrieves the wrapping difference between 2 u16 values
/// wrapping_diff(1,2) will return 1
/// wrapping_diff(2,1) will return -1
/// wrapping_diff(65535,0) will return 1
/// wrapping_diff(0,65535) will return -1
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
    use crate::wrapping_number::wrapping_diff;

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
