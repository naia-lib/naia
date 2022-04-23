use js_sys::Math::random;

/// Container for cross-platform Random methods

pub struct Random {}

impl Random {
    /// returns a random f32 value between an upper & lower bound
    pub fn gen_range_f32(lower: f32, upper: f32) -> f32 {
        let rand_range: f32 = random() as f32 * (upper - lower);
        rand_range + lower
    }

    /// returns a random u32 value between an upper & lower bound
    pub fn gen_range_u32(lower: u32, upper: u32) -> u32 {
        let rand_range: u32 = (random() * f64::from(upper - lower)) as u32;
        rand_range + lower
    }

    /// returns a random boolean value between an upper & lower bound
    pub fn gen_bool() -> bool {
        random() < 0.5
    }
}
