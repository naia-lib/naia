use snap::raw::{max_compress_len, Encoder as SnapEncoder};

pub struct Encoder {
    buffer: Vec<u8>,
    encoder: SnapEncoder,
}

impl Encoder {
    pub fn new() -> Self {
        Self {
            buffer: Vec::new(),
            encoder: SnapEncoder::new(),
        }
    }

    pub fn compress(&mut self, payload: &[u8]) -> &[u8] {
        // TODO: only use compressed packet if the resulting size would be less!
        self.buffer.resize(max_compress_len(payload.len()), 0);
        self.encoder.compress(payload, &mut self.buffer[..]);
        &self.buffer
    }
}
