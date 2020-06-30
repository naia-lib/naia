use std::io::Cursor;

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

use super::property::Property;

/// A Property that can read/write itself from/into incoming/outgoing packets
pub trait PropertyIo<T> {
    /// Given a cursor into incoming packet data, updates the Property with the
    /// synced value
    fn read(&mut self, cursor: &mut Cursor<&[u8]>);
    /// Writes contained value into outgoing byte stream
    fn write(&self, buffer: &mut Vec<u8>);
}

//// Non-Primitive Implementations ////

//String
impl PropertyIo<String> for Property<String> {
    fn read(&mut self, cursor: &mut Cursor<&[u8]>) {
        let length = cursor.read_u8().unwrap();
        let buffer = &mut Vec::with_capacity(length as usize);
        for _ in 0..length {
            buffer.push(cursor.read_u8().unwrap());
        }
        self.inner = String::from_utf8_lossy(buffer).to_string();
    }

    fn write(&self, buffer: &mut Vec<u8>) {
        buffer.push(self.inner.len() as u8);
        let mut bytes = self.inner.as_bytes().to_vec();
        buffer.append(&mut bytes);
    }
}

//// Primitive Implementations ////

//u8
impl PropertyIo<u8> for Property<u8> {
    fn read(&mut self, cursor: &mut Cursor<&[u8]>) {
        self.inner = cursor.read_u8().unwrap();
    }

    fn write(&self, buffer: &mut Vec<u8>) {
        buffer.push(self.inner);
    }
}

//u16
impl PropertyIo<u16> for Property<u16> {
    fn read(&mut self, cursor: &mut Cursor<&[u8]>) {
        self.inner = cursor.read_u16::<BigEndian>().unwrap();
    }

    fn write(&self, buffer: &mut Vec<u8>) {
        buffer.write_u16::<BigEndian>(self.inner).unwrap();
    }
}

//u32
impl PropertyIo<u32> for Property<u32> {
    fn read(&mut self, cursor: &mut Cursor<&[u8]>) {
        self.inner = cursor.read_u32::<BigEndian>().unwrap();
    }

    fn write(&self, buffer: &mut Vec<u8>) {
        buffer.write_u32::<BigEndian>(self.inner).unwrap();
    }
}

//u64
impl PropertyIo<u64> for Property<u64> {
    fn read(&mut self, cursor: &mut Cursor<&[u8]>) {
        self.inner = cursor.read_u64::<BigEndian>().unwrap();
    }

    fn write(&self, buffer: &mut Vec<u8>) {
        buffer.write_u64::<BigEndian>(self.inner).unwrap();
    }
}

//u128
impl PropertyIo<u128> for Property<u128> {
    fn read(&mut self, cursor: &mut Cursor<&[u8]>) {
        self.inner = cursor.read_u128::<BigEndian>().unwrap();
    }

    fn write(&self, buffer: &mut Vec<u8>) {
        buffer.write_u128::<BigEndian>(self.inner).unwrap();
    }
}

//i8
impl PropertyIo<i8> for Property<i8> {
    fn read(&mut self, cursor: &mut Cursor<&[u8]>) {
        self.inner = cursor.read_i8().unwrap();
    }

    fn write(&self, buffer: &mut Vec<u8>) {
        buffer.write_i8(self.inner).unwrap();
    }
}

//i16
impl PropertyIo<i16> for Property<i16> {
    fn read(&mut self, cursor: &mut Cursor<&[u8]>) {
        self.inner = cursor.read_i16::<BigEndian>().unwrap();
    }

    fn write(&self, buffer: &mut Vec<u8>) {
        buffer.write_i16::<BigEndian>(self.inner).unwrap();
    }
}

//i32
impl PropertyIo<i32> for Property<i32> {
    fn read(&mut self, cursor: &mut Cursor<&[u8]>) {
        self.inner = cursor.read_i32::<BigEndian>().unwrap();
    }

    fn write(&self, buffer: &mut Vec<u8>) {
        buffer.write_i32::<BigEndian>(self.inner).unwrap();
    }
}

//i64
impl PropertyIo<i64> for Property<i64> {
    fn read(&mut self, cursor: &mut Cursor<&[u8]>) {
        self.inner = cursor.read_i64::<BigEndian>().unwrap();
    }

    fn write(&self, buffer: &mut Vec<u8>) {
        buffer.write_i64::<BigEndian>(self.inner).unwrap();
    }
}

//i128
impl PropertyIo<i128> for Property<i128> {
    fn read(&mut self, cursor: &mut Cursor<&[u8]>) {
        self.inner = cursor.read_i128::<BigEndian>().unwrap();
    }

    fn write(&self, buffer: &mut Vec<u8>) {
        buffer.write_i128::<BigEndian>(self.inner).unwrap();
    }
}

//f32
impl PropertyIo<f32> for Property<f32> {
    fn read(&mut self, cursor: &mut Cursor<&[u8]>) {
        self.inner = cursor.read_f32::<BigEndian>().unwrap();
    }

    fn write(&self, buffer: &mut Vec<u8>) {
        buffer.write_f32::<BigEndian>(self.inner).unwrap();
    }
}

//f64
impl PropertyIo<f64> for Property<f64> {
    fn read(&mut self, cursor: &mut Cursor<&[u8]>) {
        self.inner = cursor.read_f64::<BigEndian>().unwrap();
    }

    fn write(&self, buffer: &mut Vec<u8>) {
        buffer.write_f64::<BigEndian>(self.inner).unwrap();
    }
}

//char
impl PropertyIo<char> for Property<char> {
    fn read(&mut self, cursor: &mut Cursor<&[u8]>) {
        self.inner = std::char::from_u32(cursor.read_u32::<BigEndian>().unwrap()).unwrap();
    }

    fn write(&self, buffer: &mut Vec<u8>) {
        buffer.write_u32::<BigEndian>(self.inner as u32).unwrap();
    }
}

//bool
impl PropertyIo<bool> for Property<bool> {
    fn read(&mut self, cursor: &mut Cursor<&[u8]>) {
        self.inner = cursor.read_u8().unwrap() == 1;
    }

    fn write(&self, buffer: &mut Vec<u8>) {
        buffer.write_u8(if self.inner { 1 } else { 0 }).unwrap();
    }
}
