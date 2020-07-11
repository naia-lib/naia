/// Manages the current tick on the host
pub trait HostTickManager {
    /// Gets the current tick on the host
    fn get_tick(&self) -> u16;

    /// Processes the tick latency resulting from an incoming packet header
    fn process_incoming(&mut self, tick_latency: i8);
}
