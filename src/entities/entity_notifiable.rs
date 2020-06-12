pub trait EntityNotifiable {
    fn notify_packet_delivered(&mut self, packet_index: u16);
    fn notify_packet_dropped(&mut self, packet_index: u16);
}