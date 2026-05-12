use js_sys::Date;

#[doc(hidden)]
pub struct Timestamp;

impl Timestamp {
    /// Returns the current wall-clock time as milliseconds since the Unix epoch.
    pub fn now() -> u64 {
        Date::now() as u64
    }
}
