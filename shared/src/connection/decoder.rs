cfg_if! {
    if #[cfg(feature = "zstd_support")]
    {
        use zstd::bulk::Decompressor;

        use super::compression_config::CompressionMode;

        #[derive(Clone)]
        pub struct Decoder {
            result: Vec<u8>,
            decoder: Option<Decompressor<'static>>,
        }

        impl Decoder {
            pub fn new(compression_mode: CompressionMode) -> Self {
                let decoder = match compression_mode {
                    CompressionMode::Training(_) => None,
                    CompressionMode::Default(_) => {
                        Some(Decompressor::new().expect("error creating Decompressor"))
                    }
                    CompressionMode::Dictionary(_, dictionary) => Some(
                        Decompressor::with_dictionary(&dictionary).expect("error creating Decompressor"),
                    ),
                };

                Self {
                    decoder,
                    result: Vec::new(),
                }
            }

            pub fn decode(&mut self, payload: &[u8]) -> &[u8] {
                // First byte is the is_compressed flag (written by encoder)
                let (is_compressed, data) = match payload.split_first() {
                    Some((&flag, rest)) => (flag != 0, rest),
                    None => {
                        self.result = Vec::new();
                        return &self.result;
                    }
                };

                if is_compressed {
                    if let Some(decoder) = &mut self.decoder {
                        self.result = decoder
                            .decompress(
                                data,
                                Decompressor::<'static>::upper_bound(data)
                                    .expect("upper bound decode error"),
                            )
                            .expect("decode error");
                        return &self.result;
                    }
                }
                // Not compressed (or no decoder configured): return raw data
                self.result = data.to_vec();
                &self.result
            }
        }
    }
    else
    {
        use super::compression_config::CompressionMode;

        /// Packet decoder (no-op variant: passes payload through unchanged).
        #[derive(Clone)]
        pub struct Decoder {
            result: Vec<u8>,
        }

        impl Decoder {
            /// Creates a no-op decoder (compression mode is ignored in this build variant).
            pub fn new(_: CompressionMode) -> Self {
                Self {
                    result: Vec::new(),
                }
            }

            /// Returns the payload unchanged (no-op decompression).
            pub fn decode(&mut self, payload: &[u8]) -> &[u8] {
                self.result = payload.to_vec();
                &self.result
            }
        }
    }
}
