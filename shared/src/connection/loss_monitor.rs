const WINDOW: usize = 64;

/// Rolling packet-loss estimator over the last 64 resolved packets.
/// Tracks outcomes (acked vs lost) for `PacketType::Data` packets only;
/// heartbeats and other non-data packets are excluded by the caller.
///
/// O(1) record and O(1) query. Memory cost: 64 bytes + 3 words.
pub struct LossMonitor {
    outcomes: [bool; WINDOW], // true = acked, false = lost
    write_pos: usize,
    total: usize,      // number of valid entries, capped at WINDOW
    acked_count: usize, // number of acked entries in the valid window
}

impl Default for LossMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl LossMonitor {
    /// Creates a new `LossMonitor` with a zeroed sliding window.
    pub fn new() -> Self {
        Self {
            outcomes: [false; WINDOW],
            write_pos: 0,
            total: 0,
            acked_count: 0,
        }
    }

    /// Records that a packet was acknowledged (not lost).
    pub fn record_acked(&mut self) {
        self.record(true);
    }

    /// Records that a packet was lost (not acknowledged).
    pub fn record_lost(&mut self) {
        self.record(false);
    }

    fn record(&mut self, acked: bool) {
        if self.total == WINDOW {
            // Evict oldest outcome from running tally before overwriting.
            if self.outcomes[self.write_pos] {
                self.acked_count -= 1;
            }
        } else {
            self.total += 1;
        }
        self.outcomes[self.write_pos] = acked;
        if acked {
            self.acked_count += 1;
        }
        self.write_pos = (self.write_pos + 1) % WINDOW;
    }

    /// Fraction of tracked packets that were lost (0.0–1.0).
    /// Returns 0.0 if no packets have been tracked yet.
    pub fn packet_loss_pct(&self) -> f32 {
        if self.total == 0 {
            return 0.0;
        }
        1.0 - (self.acked_count as f32 / self.total as f32)
    }
}

#[cfg(test)]
mod tests {
    use super::LossMonitor;

    #[test]
    fn empty_returns_zero() {
        let m = LossMonitor::new();
        assert_eq!(m.packet_loss_pct(), 0.0);
    }

    #[test]
    fn all_acked_returns_zero_loss() {
        let mut m = LossMonitor::new();
        for _ in 0..32 {
            m.record_acked();
        }
        assert_eq!(m.packet_loss_pct(), 0.0);
    }

    #[test]
    fn all_lost_returns_one() {
        let mut m = LossMonitor::new();
        for _ in 0..64 {
            m.record_lost();
        }
        assert_eq!(m.packet_loss_pct(), 1.0);
    }

    #[test]
    fn fifty_percent_loss() {
        let mut m = LossMonitor::new();
        for i in 0..64 {
            if i % 2 == 0 { m.record_acked(); } else { m.record_lost(); }
        }
        assert!((m.packet_loss_pct() - 0.5).abs() < 1e-6);
    }

    #[test]
    fn window_evicts_oldest_entries() {
        let mut m = LossMonitor::new();
        // Fill with 64 losses, then add 64 acks — loss should drop to 0.
        for _ in 0..64 { m.record_lost(); }
        assert_eq!(m.packet_loss_pct(), 1.0);
        for _ in 0..64 { m.record_acked(); }
        assert_eq!(m.packet_loss_pct(), 0.0);
    }
}
