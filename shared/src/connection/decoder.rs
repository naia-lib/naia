cfg_if! {
    if #[cfg(feature = "zstd_support")]
    {
        use zstd::bulk::Decompressor;

        use super::compression_config::CompressionMode;

        pub struct Decoder {
            mode: CompressionMode,
            result: Vec<u8>,
            decoder: Option<Decompressor<'static>>,
        }

        // Manual `Clone`: zstd's `Decompressor` is stateless across
        // `decompress` calls, so a clone rebuilds an identical decoder from
        // the stored mode instead of copying handles.
        impl Clone for Decoder {
            fn clone(&self) -> Self {
                let mut out = Self::new(self.mode.clone());
                out.result = self.result.clone();
                out
            }
        }

        impl Decoder {
            // Local-only: the constructor takes locally built config, never
            // wire bytes; a failing zstd handle is a local init error.
            pub fn new(compression_mode: CompressionMode) -> Self {
                let decoder = match &compression_mode {
                    CompressionMode::Training(_) => None,
                    CompressionMode::Default(_) => {
                        Some(Decompressor::new().expect("error creating Decompressor"))
                    }
                    CompressionMode::Dictionary(_, dictionary) => Some(
                        Decompressor::with_dictionary(dictionary).expect("error creating Decompressor"),
                    ),
                };

                Self {
                    mode: compression_mode,
                    decoder,
                    result: Vec::new(),
                }
            }

            /// Decodes one inbound packet body. Returns `None` when the bytes
            /// are not decodable (flag says compressed but the body is no
            /// zstd frame); the io layer drops such packets per [common-02].
            /// Paths that never panicked (empty input, uncompressed or
            /// decoder-less passthrough, successful decompress) keep their
            /// exact behavior behind `Some`.
            pub fn decode(&mut self, payload: &[u8]) -> Option<&[u8]> {
                // First byte is the is_compressed flag (written by encoder)
                let (is_compressed, data) = match payload.split_first() {
                    Some((&flag, rest)) => (flag != 0, rest),
                    None => {
                        self.result = Vec::new();
                        return Some(&self.result);
                    }
                };

                if is_compressed {
                    if let Some(decoder) = &mut self.decoder {
                        // [115662-2] common-02: corrupt compressed bytes fail
                        // the read instead of panicking on the expects.
                        let bound = Decompressor::<'static>::upper_bound(data)?;
                        self.result = decoder.decompress(data, bound).ok()?;
                        return Some(&self.result);
                    }
                }
                // Not compressed (or no decoder configured): return raw data
                self.result = data.to_vec();
                Some(&self.result)
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
            #[must_use]
            pub fn new(_: CompressionMode) -> Self {
                Self {
                    result: Vec::new(),
                }
            }

            /// Returns the payload unchanged (no-op decompression).
            /// `Option` only to match the zstd variant's signature; always `Some`.
            pub fn decode(&mut self, payload: &[u8]) -> Option<&[u8]> {
                self.result = payload.to_vec();
                Some(&self.result)
            }
        }
    }
}

// ---- [115662-2] corrupt-compressed-payload probe: the flag byte says
// compressed but the body is not a zstd frame. Pre-fix this panics inside
// `decode` (`upper_bound` / `decompress` expects); post-fix it decodes to
// `None` so the io layer drops the packet.
#[cfg(all(test, feature = "zstd_support"))]
mod corrupt_payload_tests {
    use super::super::compression_config::CompressionMode;
    use super::Decoder;

    // [115662-2] GREEN: the flag byte says compressed but the body is no
    // zstd frame. Pre-fix this panicked (`upper bound decode error`
    // expect at :60); post-fix it decodes to `None`.
    #[test]
    fn corrupt_compressed_payload_probe() {
        // common-02: corrupt inbound bytes decode to `None` (the io layer
        // drops the packet) instead of panicking.
        let mut decoder = Decoder::new(CompressionMode::Default(0));
        let payload = [1u8, 0xDE, 0xAD, 0xBE, 0xEF];
        assert!(
            decoder.decode(&payload).is_none(),
            "corrupt compressed bytes must decode to None, never panic",
        );
    }
}
