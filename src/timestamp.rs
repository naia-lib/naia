
use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

cfg_if! {
    if #[cfg(target_arch = "wasm32")] {
        // Wasm //
        use js_sys::Date;
    }
    else {
        // Linux //
        use std::time::SystemTime;
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Timestamp {
    time: u64,
}

impl Timestamp {
    pub fn now() -> Self {

        cfg_if! {
            if #[cfg(target_arch = "wasm32")] {
                // Wasm //
                Timestamp {
                    time: Date::now() as u64
                }
            }
            else {
                // Linux //
                let time = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)
                    .expect("timing error!")
                    .as_secs();
                Timestamp {
                    time
                }
            }
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