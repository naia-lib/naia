use zstd::bulk::Decompressor;

pub struct Decoder {
    result: Vec<u8>,
    decoder: Decompressor<'static>,
}

impl Decoder {
    pub fn new() -> Self {
        Self {
            decoder: Decompressor::new().expect("error creating Decompressor"),
            result: Vec::new(),
        }
    }

    pub fn decode(&mut self, payload: &[u8]) -> &[u8] {
        self.result = self
            .decoder
            .decompress(
                payload,
                Decompressor::<'static>::upper_bound(payload).expect("upper bound decode error"),
            )
            .expect("decode error");
        &self.result
    }
}
