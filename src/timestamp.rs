
use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

use log::{info};

use std::io::Read;

use crate::PacketType;

cfg_if! {
    if #[cfg(target_arch = "wasm32")] {
        // Wasm //
        use js_sys::Date;
    }
    else {
        // Linux //
        use std::time::{Duration, SystemTime};
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Timestamp {
    time: u64,
}

impl Timestamp {
    pub fn now() -> Self {
        let mut time: u64 = 0;

        cfg_if! {
            if #[cfg(target_arch = "wasm32")] {
                // Wasm //
                time = Date::now() as u64;
            }
            else {
                // Linux //
                time = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)
                    .expect("timing error!")
                    .as_secs();
            }
        }

        Timestamp {
            time
        }
    }

    pub fn write(&self, buffer: &mut Vec<u8>) {
        buffer.write_u64::<BigEndian>(self.time).unwrap();
    }

    pub fn read(mut msg: &[u8]) -> Self {

        let time = msg.read_u64::<BigEndian>().unwrap();

        Timestamp {
            time
        }
    }
}