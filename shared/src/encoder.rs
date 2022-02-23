use std::fs;

use log::info;

use zstd::{bulk::Compressor, dict::from_continuous};

use super::compression_config::CompressionMode;

pub struct Encoder {
    result: Vec<u8>,
    encoder: Compressor<'static>,
    dictionary_trainer: Option<DictionaryTrainer>,
}

impl Encoder {
    pub fn new(compression_mode: CompressionMode) -> Self {
        let dictionary_trainer = match compression_mode {
            CompressionMode::Training(dict_size) => Some(DictionaryTrainer::new(dict_size)),
            CompressionMode::Regular => None,
        };

        Self {
            result: Vec::new(),
            encoder: Compressor::new(3).expect("error creating Compressor"),
            dictionary_trainer,
        }
    }

    pub fn encode(&mut self, payload: &[u8]) -> &[u8] {
        // TODO: only use compressed packet if the resulting size would be less!
        if let Some(trainer) = &mut self.dictionary_trainer {
            trainer.record_bytes(payload);
            self.result = payload.to_vec();
            &self.result
        } else {
            self.result = self.encoder.compress(payload).expect("encode error");
            &self.result
        }
    }
}

pub struct DictionaryTrainer {
    sample_data: Vec<u8>,
    sample_sizes: Vec<usize>,
    next_alert_length: usize,
    target_sample_size: usize,
    training_complete: bool,
    /// The desired size of the output dictionary (in Bytes)
    target_dict_size: usize
}

impl DictionaryTrainer {
    /// `dictionary_size` here describes the desired size of the dictionary (in Kilobytes).
    /// Obviously, the bigger the dictionary the better theoretical compression.
    pub fn new(dictionary_size: usize) -> Self {
        Self {
            target_dict_size: dictionary_size * 1000,
            target_sample_size: dictionary_size * 1000 * 100,
            sample_data: Vec::new(),
            sample_sizes: Vec::new(),
            next_alert_length: 0,
            training_complete: false,
        }
    }

    pub fn record_bytes(&mut self, bytes: &[u8]) {
        if self.training_complete {
            return;
        }

        self.sample_data.extend_from_slice(bytes);
        self.sample_sizes.push(bytes.len());

        let current_sample_size = self.sample_data.len();

        if current_sample_size >= self.next_alert_length {
            let percent = ((current_sample_size as f32) / (self.target_sample_size as f32)) * 100.0;
            info!("Dictionary training: {}% complete", percent);

            self.next_alert_length += self.target_dict_size * 5;
        }

        if self.sample_data.len() >= self.target_sample_size {
            info!("Dictionary training complete!");
            info!("Samples: {} ({} KB)", self.sample_sizes.len(), self.sample_data.len());
            info!("Dictionary processing sample data...");

            // We have enough sample data to train the dictionary!
            let dictionary = from_continuous(&self.sample_data, &self.sample_sizes, self.target_dict_size).expect("Error while training dictionary");

            // Now need to ... write it to a file I guess
            fs::write("dictionary.txt", dictionary).expect("Error while writing dictionary to file");

            info!("Dictionary written to `dictionary.txt`!");
        }
    }
}
