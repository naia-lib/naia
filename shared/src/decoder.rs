use snap::raw::{decompress_len, Decoder as SnapDecoder};

pub struct Decoder {
    buffer: Vec<u8>,
    decoder: SnapDecoder,
}

impl Decoder {
    pub fn new() -> Self {
        Self {
            buffer: Vec::new(),
            decoder: SnapDecoder::new(),
        }
    }

    pub fn decompress(&mut self, payload: &[u8]) -> &[u8] {
        // TODO: only use compressed packet if the resulting size would be less!
        self.buffer.resize(decompress_len(payload).unwrap(), 0);
        self.decoder.decompress(payload, &mut self.buffer);
        &self.buffer
    }
}
