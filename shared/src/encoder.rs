use zstd::bulk::Compressor;

pub struct Encoder {
    result: Vec<u8>,
    encoder: Compressor<'static>,
}

impl Encoder {
    pub fn new() -> Self {
        Self {
            result: Vec::new(),
            encoder: Compressor::new(3).expect("error creating Compressor"),
        }
    }

    pub fn encode(&mut self, payload: &[u8]) -> &[u8] {
        // TODO: only use compressed packet if the resulting size would be less!
        self.result = self.encoder.compress(payload).expect("encode error");
        &self.result
    }
}
