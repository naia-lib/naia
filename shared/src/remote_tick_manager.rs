use crate::standard_header::StandardHeader;

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

    pub fn process_incoming(&mut self, host_tick: u16, header: &StandardHeader) {
        let remote_tick = header.tick();
        let tick_latency = header.tick_diff();
        unimplemented!()
    }
}
