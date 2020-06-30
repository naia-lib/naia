/// Represents a manager that must be notified when packets have been dropped or
/// delivered
pub trait EntityNotifiable {
    /// Notifies the manager that a packet has been delivered
    fn notify_packet_delivered(&mut self, packet_index: u16);
    /// Notifies the manager that a packet has been dropped
    fn notify_packet_dropped(&mut self, packet_index: u16);
}
