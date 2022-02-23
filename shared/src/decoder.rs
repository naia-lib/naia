use zstd::bulk::Decompressor;

use super::compression_config::CompressionMode;

pub struct Decoder {
    result: Vec<u8>,
    decoder: Decompressor<'static>,
    training: bool,
}

impl Decoder {
    pub fn new(compression_mode: CompressionMode) -> Self {
        Self {
            decoder: Decompressor::new().expect("error creating Decompressor"),
            result: Vec::new(),
            training: compression_mode.is_training(),
        }
    }

    pub fn decode(&mut self, payload: &[u8]) -> &[u8] {
        if self.training {
            self.result = payload.to_vec();
            &self.result
        } else {
            self.result = self
                .decoder
                .decompress(
                    payload,
                    Decompressor::<'static>::upper_bound(payload)
                        .expect("upper bound decode error"),
                )
                .expect("decode error");
            &self.result
        }
    }
}
