/// Manages the current tick on the host
pub trait HostTickManager {
    /// Gets the current tick on the host
    fn get_tick(&self) -> u16;
}
