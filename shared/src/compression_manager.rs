
use snap::raw::{decompress_len, max_compress_len, Decoder as SnapDecoder, Encoder as SnapEncoder};

pub struct CompressionManager {
    buffer: Vec<u8>,
    encoder: SnapEncoder,
    decoder: SnapDecoder,
}

impl CompressionManager {
    pub fn new() -> Self {
        CompressionManager {
            buffer: Vec::new(),
            encoder: SnapEncoder::new(),
            decoder: SnapDecoder::new(),
        }
    }

    pub fn compress(&mut self, payload: &[u8]) -> &[u8] {
        // TODO: only use compressed packet if the resulting size would be less!
        self.buffer.resize(max_compress_len(payload.len()), 0);
        self.encoder.compress(payload, &mut self.buffer[..]);
        &self.buffer
    }

    pub fn decompress(&mut self, payload: &[u8]) -> &[u8] {
        // TODO: only use compressed packet if the resulting size would be less!
        self.buffer.resize(decompress_len(payload).unwrap(), 0);
        self.decoder.decompress(payload, &mut self.buffer);
        &self.buffer
    }
}