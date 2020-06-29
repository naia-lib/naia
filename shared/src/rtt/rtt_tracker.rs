use crate::{sequence_buffer::SequenceBuffer, RttData, RttMeasurer};

#[derive(Debug)]
pub struct RttTracker {
    rtt_measurer: RttMeasurer,
    congestion_data: SequenceBuffer<RttData>,
}

impl RttTracker {
    pub fn new(rtt_smoothing_factor: f32, rtt_max_value: u16) -> RttTracker {
        RttTracker {
            rtt_measurer: RttMeasurer::new(rtt_smoothing_factor, rtt_max_value),
            congestion_data: SequenceBuffer::with_capacity(<u16>::max_value()),
        }
    }

    pub fn process_incoming(&mut self, incoming_seq: u16) {
        let congestion_data = self.congestion_data.get_mut(incoming_seq);
        self.rtt_measurer.calculate_rrt(congestion_data);
    }

    pub fn process_outgoing(&mut self, seq: u16) {
        self.congestion_data.insert(seq, RttData::new(seq));
    }

    pub fn get_rtt(&self) -> f32 {
        return self.rtt_measurer.get_rtt();
    }
}
