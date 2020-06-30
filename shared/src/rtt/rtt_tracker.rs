use crate::{
    rtt::{rtt_data::RttData, rtt_measurer::RttMeasurer},
    sequence_buffer::SequenceBuffer,
};

/// Tracks the current Round Trip Time of the connection
#[derive(Debug)]
pub struct RttTracker {
    rtt_measurer: RttMeasurer,
    rtt_data: SequenceBuffer<RttData>,
}

impl RttTracker {
    /// Creates a new RttTracker which is used to keep track of the Round Trip
    /// Time of a connection
    pub fn new(rtt_smoothing_factor: f32, rtt_max_value: u16) -> RttTracker {
        RttTracker {
            rtt_measurer: RttMeasurer::new(rtt_smoothing_factor, rtt_max_value),
            rtt_data: SequenceBuffer::with_capacity(<u16>::max_value()),
        }
    }

    /// Process an incoming packet, calculates Round Trip Time
    pub fn process_incoming(&mut self, incoming_seq: u16) {
        let rtt_data = self.rtt_data.get_mut(incoming_seq);
        self.rtt_measurer.calculate_rrt(rtt_data);
    }

    /// Process an outgoing packet, recording the time it was sent in order to
    /// measure time elapsed to a response
    pub fn process_outgoing(&mut self, seq: u16) {
        self.rtt_data.insert(seq, RttData::new(seq));
    }

    /// Get the current measured Round Trip Time for the connection
    pub fn get_rtt(&self) -> f32 {
        return self.rtt_measurer.get_rtt();
    }
}
