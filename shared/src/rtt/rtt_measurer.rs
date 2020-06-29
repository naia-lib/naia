use crate::{Duration, RttData};

#[derive(Debug)]
pub struct RttMeasurer {
    rtt_smoothing_factor: f32,
    rtt_max_value: u16,
    rtt: f32,
}

impl RttMeasurer {
    pub fn new(rtt_smoothing_factor: f32, rtt_max_value: u16) -> RttMeasurer {
        RttMeasurer {
            rtt_smoothing_factor,
            rtt_max_value,
            rtt: 0.,
        }
    }

    pub fn calculate_rrt(&mut self, rtt_data: Option<&mut RttData>) {
        self.rtt = self.get_smoothed_rtt(rtt_data);
    }

    pub fn get_rtt(&self) -> f32 {
        return self.rtt;
    }

    /// This will get the smoothed round trip time (rtt) from the time we last heard from a packet.
    fn get_smoothed_rtt(&self, rtt_entry: Option<&mut RttData>) -> f32 {
        match rtt_entry {
            Some(avoidance_data) => {
                let elapsed_time = avoidance_data.sending_time.elapsed();

                let rtt_time = self.as_milliseconds(elapsed_time);

                self.smooth_out_rtt(rtt_time)
            }
            None => 0.0,
        }
    }

    /// Converts a duration to milliseconds.
    ///
    /// `as_milliseconds` is not supported yet supported in rust stable.
    /// See this stackoverflow post for more info: https://stackoverflow.com/questions/36816072/how-do-i-get-a-duration-as-a-number-of-milliseconds-in-rust
    fn as_milliseconds(&self, duration: Duration) -> u64 {
        let nanos = u64::from(duration.subsec_nanos());
        (1000 * 1000 * 1000 * duration.as_secs() + nanos) / (1000 * 1000)
    }

    /// Smooths out round trip time (rtt) value by the specified smoothing factor.
    ///
    /// First we subtract the max allowed rtt.
    /// This way we can see by how many we are off from the max allowed rtt.
    /// Then we multiply with or smoothing factor.
    ///
    /// We do this so that if one packet has an bad rtt it will not directly bring down the or network quality estimation.
    /// The default is 10% smoothing so if in total or packet is 50 milliseconds later than max allowed rtt we will increase or rtt estimation with 5.
    fn smooth_out_rtt(&self, rtt: u64) -> f32 {
        let exceeded_rrt_time = rtt as i64 - i64::from(self.rtt_max_value);
        exceeded_rrt_time as f32 * self.rtt_smoothing_factor
    }
}
