use crate::{BitReader, BitWriter};
use crate::error::DeErr;
use crate::traits::{De, Ser};

pub type UnsignedInteger<const BITS: u8> = SerdeInteger<false, false, BITS>;
pub type SignedInteger<const BITS: u8> = SerdeInteger<true, false, BITS>;
pub type UnsignedVariableInteger<const BITS: u8> = SerdeInteger<false, true, BITS>;
pub type SignedVariableInteger<const BITS: u8>  = SerdeInteger<true, true, BITS>;

#[derive(Debug, Eq, PartialEq)]
pub struct SerdeInteger<const SIGNED: bool, const VARIABLE: bool, const BITS: u8> {
    inner: i128
}

impl<const SIGNED: bool, const VARIABLE: bool, const BITS: u8> SerdeInteger<SIGNED, VARIABLE, BITS> {
    pub fn new<T: Into<i128>>(value: T) -> Self {

        let inner = Into::<i128>::into(value);

        if inner < 0 && !SIGNED {
            panic!("can't encode a negative number with an Unsigned Integer!");
        }

        if BITS == 0 {
            panic!("can't create an integer with 0 bits...");
        }
        if BITS > 127 {
            panic!("can't create an integer with more than 127 bits...");
        }

        if !VARIABLE {
            let max_value: i128 = 2_i128.pow(BITS as u32);
            if inner >= max_value {
                panic!("with {} bits, can't encode number greater than {}", BITS, max_value);
            }
            if inner < 0 && SIGNED {
                let min_value: i128 = (2_i128.pow(BITS as u32)) * -1;
                if inner <= min_value {
                    panic!("with {} bits, can't encode number less than {}", BITS, min_value);
                }
            }
        }

        Self {
            inner,
        }
    }

    fn new_unchecked(value: i128) -> Self {
        Self {
            inner: value,
        }
    }
}

impl<const SIGNED: bool, const VARIABLE: bool, const BITS: u8> Ser for SerdeInteger<SIGNED, VARIABLE, BITS> {
    fn ser(&self, writer: &mut BitWriter) {

        let mut value: u128;
        let negative = self.inner < 0;

        if SIGNED {
            // 1 if negative, 0 if positive
            writer.write_bit(negative);
            value = (self.inner * -1) as u128;

        } else {
            value = self.inner as u128;
        }

        if VARIABLE {
            let mut proceed;
            loop {
                if value >= 2_u128.pow(BITS as u32) {
                    proceed = true;
                } else {
                    proceed = false;
                }
                writer.write_bit(proceed);

                for _ in 0..BITS {
                    writer.write_bit(value & 1 != 0);
                    value = value >> 1;
                }
                if !proceed {
                    return;
                }
            }
        } else {
            for _ in 0..BITS {
                writer.write_bit(value & 1 != 0);
                value = value >> 1;
            }
            return;
        }
    }
}

impl<const SIGNED: bool, const VARIABLE: bool, const BITS: u8> De for SerdeInteger<SIGNED, VARIABLE, BITS> {
    fn de(reader: &mut BitReader) -> Result<Self, DeErr> {

        let mut negative: bool = false;
        if SIGNED {
            negative = reader.read_bit();
        }

        if VARIABLE {

            let mut total_bits: usize = 0;
            let mut output: u128 = 0;

            loop {
                let proceed = reader.read_bit();

                for _ in 0..BITS {
                    total_bits += 1;

                    output = output << 1;

                    if reader.read_bit() {
                        output = output | 1;
                    }
                }

                if !proceed {

                    output = output << (128 - total_bits);
                    output = output.reverse_bits();

                    let value: i128 = output as i128;
                    if negative {
                        return Ok(SerdeInteger::new_unchecked(value * -1));
                    } else {
                        return Ok(SerdeInteger::new_unchecked(value));
                    }
                }
            }
        } else {

            let mut output: u128 = 0;

            for _ in 0..BITS {

                output = output << 1;

                if reader.read_bit() {
                    output = output | 1;
                }
            }

            output = output << (128 - BITS);
            output = output.reverse_bits();

            let value: i128 = output as i128;
            if negative {
                return Ok(SerdeInteger::new_unchecked(value * -1));
            } else {
                return Ok(SerdeInteger::new_unchecked(value));
            }
        }
    }
}