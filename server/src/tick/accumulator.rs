use std::time::{Duration, Instant};

use naia_shared::Tick;

pub struct Accumulator {
    delta: f32,
    duration_millis: f32,
    last: Instant,
}

impl Accumulator {
    pub fn new(duration: Duration) -> Self {
        Accumulator {
            delta: 0.0,
            last: Instant::now(),
            duration_millis: duration.as_millis() as f32,
        }
    }

    pub fn take_ticks(&mut self) -> Tick {
        let mut output_ticks: Tick = 0;
        let frame_millis = self.last.elapsed().as_nanos() as f32 / 1000000.0;
        self.delta += frame_millis;
        self.last = Instant::now();
        while self.delta >= self.duration_millis {
            self.delta -= self.duration_millis;
            output_ticks = output_ticks.wrapping_add(1);
        }
        output_ticks
    }
}
