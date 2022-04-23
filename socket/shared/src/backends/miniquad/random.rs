extern "C" {
    pub fn naia_random() -> f64;
}

/// Container for cross-platform Random methods

pub struct Random {}

impl Random {
    /// returns a random f32 value between an upper & lower bound
    pub fn gen_range_f32(lower: f32, upper: f32) -> f32 {
        unsafe {
            let rand_range: f32 = naia_random() as f32 * (upper - lower);
            rand_range + lower
        }
    }

    /// returns a random u32 value between an upper & lower bound
    pub fn gen_range_u32(lower: u32, upper: u32) -> u32 {
        unsafe {
            let rand_range: u32 = (naia_random() * f64::from(upper - lower)) as u32;
            rand_range + lower
        }
    }

    /// returns a random boolean value between an upper & lower bound
    pub fn gen_bool() -> bool {
        unsafe { naia_random() < 0.5 }
    }
}
