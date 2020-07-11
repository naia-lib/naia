#[derive(Debug)]
pub struct RemoteTickManager {
    tick_latency: u8,
}

impl RemoteTickManager {
    pub fn new() -> Self {
        RemoteTickManager { tick_latency: 0 }
    }

    pub fn get_tick_latency(&self) -> u8 {
        self.tick_latency
    }
}
