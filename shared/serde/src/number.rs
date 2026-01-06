use crate::{
    bit_reader::BitReader, bit_writer::BitWrite, error::SerdeErr, serde::Serde, ConstBitLength,
};

// Integers

pub trait SerdeIntegerConversion<const SIGNED: bool, const VARIABLE: bool, const BITS: u8> {
    fn from(value: &SerdeInteger<SIGNED, VARIABLE, BITS>) -> Self;
}

pub type UnsignedInteger<const BITS: u8> = SerdeInteger<false, false, BITS>;
pub type SignedInteger<const BITS: u8> = SerdeInteger<true, false, BITS>;
pub type UnsignedVariableInteger<const BITS: u8> = SerdeInteger<false, true, BITS>;
pub type SignedVariableInteger<const BITS: u8> = SerdeInteger<true, true, BITS>;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub struct SerdeInteger<const SIGNED: bool, const VARIABLE: bool, const BITS: u8> {
    inner: SerdeNumberInner,
}

impl<const SIGNED: bool, const VARIABLE: bool, const BITS: u8>
    SerdeInteger<SIGNED, VARIABLE, BITS>
{
    pub fn new<T: Into<i128>>(value: T) -> Self {
        Self {
            inner: SerdeNumberInner::new(SIGNED, VARIABLE, BITS, 0, value.into()),
        }
    }

    pub fn get(&self) -> i128 {
        self.inner.get()
    }

    pub fn set<T: Into<i128>>(&mut self, value: T) {
        self.inner.set(value.into());
    }

    pub fn to<T: SerdeIntegerConversion<SIGNED, VARIABLE, BITS>>(&self) -> T {
        T::from(self)
    }
}

impl<const SIGNED: bool, const VARIABLE: bool, const BITS: u8> Serde
    for SerdeInteger<SIGNED, VARIABLE, BITS>
{
    fn ser(&self, writer: &mut dyn BitWrite) {
        self.inner.ser(writer);
    }

    fn de(reader: &mut BitReader) -> Result<Self, SerdeErr> {
        let inner = SerdeNumberInner::de(reader, SIGNED, VARIABLE, BITS, 0)?;
        Ok(Self { inner })
    }

    fn bit_length(&self) -> u32 {
        self.inner.bit_length()
    }
}

impl<const SIGNED: bool, const BITS: u8> ConstBitLength for SerdeInteger<SIGNED, false, BITS> {
    fn const_bit_length() -> u32 {
        let mut output: u32 = 0;
        if SIGNED {
            output += 1;
        }
        output + BITS as u32
    }
}

impl<const SIGNED: bool, const VARIABLE: bool, const BITS: u8, T: Into<i128>> From<T>
    for SerdeInteger<SIGNED, VARIABLE, BITS>
{
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

impl<const SIGNED: bool, const VARIABLE: bool, const BITS: u8, T: TryFrom<i128>>
    SerdeIntegerConversion<SIGNED, VARIABLE, BITS> for T
{
    fn from(value: &SerdeInteger<SIGNED, VARIABLE, BITS>) -> Self {
        let Ok(t_value) = T::try_from(value.get()) else {
            panic!("SerdeInteger's value is out of range to convert to this type.");
        };
        t_value
    }
}

// Floats

pub trait SerdeFloatConversion<
    const SIGNED: bool,
    const VARIABLE: bool,
    const BITS: u8,
    const FRACTION_DIGITS: u8,
>
{
    fn from(value: &SerdeFloat<SIGNED, VARIABLE, BITS, FRACTION_DIGITS>) -> Self;
}

pub type UnsignedFloat<const BITS: u8, const FRACTION_DIGITS: u8> =
    SerdeFloat<false, false, BITS, FRACTION_DIGITS>;
pub type SignedFloat<const BITS: u8, const FRACTION_DIGITS: u8> =
    SerdeFloat<true, false, BITS, FRACTION_DIGITS>;
pub type UnsignedVariableFloat<const BITS: u8, const FRACTION_DIGITS: u8> =
    SerdeFloat<false, true, BITS, FRACTION_DIGITS>;
pub type SignedVariableFloat<const BITS: u8, const FRACTION_DIGITS: u8> =
    SerdeFloat<true, true, BITS, FRACTION_DIGITS>;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub struct SerdeFloat<
    const SIGNED: bool,
    const VARIABLE: bool,
    const BITS: u8,
    const FRACTION_DIGITS: u8,
> {
    inner: SerdeNumberInner,
}

impl<const SIGNED: bool, const VARIABLE: bool, const BITS: u8, const FRACTION_DIGITS: u8>
    SerdeFloat<SIGNED, VARIABLE, BITS, FRACTION_DIGITS>
{
    pub fn new<T: Into<f32>>(value: T) -> Self {
        let float_val = value.into();
        let scale = 10f32.powi(FRACTION_DIGITS as i32);
        let scaled = (float_val * scale).round() as i128;
        let inner = SerdeNumberInner::new(SIGNED, VARIABLE, BITS, FRACTION_DIGITS, scaled);
        Self { inner }
    }

    pub fn get(&self) -> f32 {
        let scale = 10f32.powi(FRACTION_DIGITS as i32);
        (self.inner.get() as f32) / scale
    }

    pub fn set<T: Into<f32>>(&mut self, value: T) {
        let float_val = value.into();
        let scale = 10f32.powi(FRACTION_DIGITS as i32);
        let scaled = (float_val * scale).round() as i128;
        self.inner.set(scaled);
    }
}

impl<const S: bool, const V: bool, const B: u8, const F: u8> Serde for SerdeFloat<S, V, B, F> {
    fn ser(&self, writer: &mut dyn BitWrite) {
        self.inner.ser(writer);
    }

    fn de(reader: &mut BitReader) -> Result<Self, SerdeErr> {
        // read in i128, convert.
        let inner = SerdeNumberInner::de(reader, S, V, B, F)?;
        // We interpret the i128 as scaled. We just store it.
        Ok(Self { inner })
    }

    fn bit_length(&self) -> u32 {
        self.inner.bit_length()
    }
}

impl<const SIGNED: bool, const BITS: u8, const FRACTION_DIGITS: u8> ConstBitLength
    for SerdeFloat<SIGNED, false, BITS, FRACTION_DIGITS>
{
    fn const_bit_length() -> u32 {
        let mut output: u32 = 0;
        if SIGNED {
            output += 1;
        }
        output + BITS as u32
    }
}

impl<
        const SIGNED: bool,
        const VARIABLE: bool,
        const BITS: u8,
        const FRACTION_DIGITS: u8,
        T: Into<f32>,
    > From<T> for SerdeFloat<SIGNED, VARIABLE, BITS, FRACTION_DIGITS>
{
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

impl<
        const SIGNED: bool,
        const VARIABLE: bool,
        const BITS: u8,
        const FRACTION_DIGITS: u8,
        T: TryFrom<f32>,
    > SerdeFloatConversion<SIGNED, VARIABLE, BITS, FRACTION_DIGITS> for T
{
    fn from(value: &SerdeFloat<SIGNED, VARIABLE, BITS, FRACTION_DIGITS>) -> Self {
        let Ok(t_value) = T::try_from(value.get()) else {
            panic!("SerdeFloat's value is out of range to convert to this type.");
        };
        t_value
    }
}

// Inner type that is not generic (avoiding code bloat from monomorphization)

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
struct SerdeNumberInner {
    inner_value: i128,
    signed: bool,
    variable: bool,
    bits: u8,
    fraction_digits: u8,
}

impl SerdeNumberInner {
    fn new(signed: bool, variable: bool, bits: u8, fraction_digits: u8, value: i128) -> Self {
        if bits == 0 {
            panic!("can't create an number with 0 bits...");
        }
        if bits > 127 {
            panic!("can't create an number with more than 127 bits...");
        }

        if !signed && value < 0 {
            panic!("can't encode a negative number with an Unsigned type!");
        }

        if !variable {
            let max_value: i128 = 2_i128.pow(bits as u32);
            if value >= max_value {
                panic!(
                    "value `{}` is too high! (with `{}` bits, can't encode number greater than `{}`)",
                    value, bits, max_value
                );
            }
            if signed && value < 0 {
                let min_value: i128 = -(2_i128.pow(bits as u32));
                if value <= min_value {
                    panic!(
                        "value `{}` is too low! (with `{}` bits, can't encode number less than `{}`)",
                        value, bits, min_value
                    );
                }
            }
        }

        Self {
            inner_value: value,
            signed,
            variable,
            bits,
            fraction_digits,
        }
    }

    fn new_unchecked(
        signed: bool,
        variable: bool,
        bits: u8,
        fraction_digits: u8,
        value: i128,
    ) -> Self {
        Self {
            inner_value: value,
            signed,
            variable,
            bits,
            fraction_digits,
        }
    }

    fn get(&self) -> i128 {
        self.inner_value
    }

    fn set(&mut self, value: i128) {
        self.inner_value = value;
    }

    fn ser(&self, writer: &mut dyn BitWrite) {
        // replicate original ser logic
        let mut value: u128;
        let negative = self.inner_value < 0;

        if self.signed {
            writer.write_bit(negative);
            if negative {
                value = -self.inner_value as u128;
            } else {
                value = self.inner_value as u128;
            }
        } else {
            value = self.inner_value as u128;
        }

        if self.variable {
            loop {
                let proceed = value >= 2_u128.pow(self.bits as u32);
                writer.write_bit(proceed);
                for _ in 0..self.bits {
                    writer.write_bit(value & 1 != 0);
                    value >>= 1;
                }
                if !proceed {
                    return;
                }
            }
        } else {
            for _ in 0..self.bits {
                writer.write_bit(value & 1 != 0);
                value >>= 1;
            }
        }
    }

    fn de(
        reader: &mut BitReader,
        signed: bool,
        variable: bool,
        bits: u8,
        fraction_digits: u8,
    ) -> Result<Self, SerdeErr> {
        let mut negative = false;
        if signed {
            negative = reader.read_bit()?;
        }

        if variable {
            let mut total_bits: usize = 0;
            let mut output: u128 = 0;

            loop {
                let proceed = reader.read_bit()?;

                for _ in 0..bits {
                    total_bits += 1;
                    output <<= 1;
                    if reader.read_bit()? {
                        output |= 1;
                    }
                }

                if !proceed {
                    output <<= 128 - total_bits;
                    output = output.reverse_bits();
                    let value: i128 = output as i128;
                    if negative {
                        return Ok(Self::new_unchecked(
                            signed,
                            variable,
                            bits,
                            fraction_digits,
                            -value,
                        ));
                    } else {
                        return Ok(Self::new_unchecked(
                            signed,
                            variable,
                            bits,
                            fraction_digits,
                            value,
                        ));
                    }
                }
            }
        } else {
            let mut output: u128 = 0;
            for _ in 0..bits {
                output <<= 1;
                if reader.read_bit()? {
                    output |= 1;
                }
            }
            output <<= 128 - bits;
            output = output.reverse_bits();

            let value: i128 = output as i128;
            if negative {
                Ok(Self::new_unchecked(
                    signed,
                    variable,
                    bits,
                    fraction_digits,
                    -value,
                ))
            } else {
                Ok(Self::new_unchecked(
                    signed,
                    variable,
                    bits,
                    fraction_digits,
                    value,
                ))
            }
        }
    }

    fn bit_length(&self) -> u32 {
        let mut output: u32 = 0;

        if self.signed {
            output += 1; // sign bit
        }

        if self.variable {
            let mut value = self.inner_value.abs() as u128;
            loop {
                let proceed = value >= 2_u128.pow(self.bits as u32);
                output += 1; // proceed bit
                for _ in 0..self.bits {
                    output += 1;
                    value >>= 1;
                }
                if !proceed {
                    break;
                }
            }
        } else {
            output += self.bits as u32;
        }
        output
    }
}

// Tests

#[cfg(test)]
mod tests {
    use crate::{
        bit_reader::BitReader,
        bit_writer::BitWriter,
        number::{
            SignedFloat, SignedInteger, SignedVariableFloat, SignedVariableInteger, UnsignedFloat,
            UnsignedInteger, UnsignedVariableFloat, UnsignedVariableInteger,
        },
        serde::Serde,
    };

    #[test]
    fn in_and_out() {
        let in_u16: u16 = 123;
        let middle = UnsignedInteger::<9>::new(in_u16);
        let out_u16: u16 = middle.get() as u16;

        assert_eq!(in_u16, out_u16);
    }

    #[test]
    fn read_write_unsigned() {
        // Write
        let mut writer = BitWriter::new();

        let in_1 = UnsignedInteger::<7>::new(123);
        let in_2 = UnsignedInteger::<20>::new(535221);
        let in_3 = UnsignedInteger::<2>::new(3);

        in_1.ser(&mut writer);
        in_2.ser(&mut writer);
        in_3.ser(&mut writer);

        let buffer = writer.to_bytes();

        // Read
        let mut reader = BitReader::new(&buffer);

        let out_1 = Serde::de(&mut reader).unwrap();
        let out_2 = Serde::de(&mut reader).unwrap();
        let out_3 = Serde::de(&mut reader).unwrap();

        assert_eq!(in_1, out_1);
        assert_eq!(in_2, out_2);
        assert_eq!(in_3, out_3);
    }

    #[test]
    fn read_write_signed() {
        // Write
        let mut writer = BitWriter::new();

        let in_1 = SignedInteger::<10>::new(-668);
        let in_2 = SignedInteger::<20>::new(53);
        let in_3 = SignedInteger::<2>::new(-3);

        in_1.ser(&mut writer);
        in_2.ser(&mut writer);
        in_3.ser(&mut writer);

        let buffer = writer.to_bytes();

        // Read
        let mut reader = BitReader::new(&buffer);

        let out_1 = Serde::de(&mut reader).unwrap();
        let out_2 = Serde::de(&mut reader).unwrap();
        let out_3 = Serde::de(&mut reader).unwrap();

        assert_eq!(in_1, out_1);
        assert_eq!(in_2, out_2);
        assert_eq!(in_3, out_3);
    }

    #[test]
    fn read_write_unsigned_variable() {
        // Write
        let mut writer = BitWriter::new();

        let in_1 = UnsignedVariableInteger::<3>::new(23);
        let in_2 = UnsignedVariableInteger::<5>::new(153);
        let in_3 = UnsignedVariableInteger::<2>::new(3);

        in_1.ser(&mut writer);
        in_2.ser(&mut writer);
        in_3.ser(&mut writer);

        let buffer = writer.to_bytes();

        // Read
        let mut reader = BitReader::new(&buffer);

        let out_1 = Serde::de(&mut reader).unwrap();
        let out_2 = Serde::de(&mut reader).unwrap();
        let out_3 = Serde::de(&mut reader).unwrap();

        assert_eq!(in_1, out_1);
        assert_eq!(in_2, out_2);
        assert_eq!(in_3, out_3);
    }

    #[test]
    fn read_write_signed_variable() {
        // Write
        let mut writer = BitWriter::new();

        let in_1 = SignedVariableInteger::<5>::new(-668);
        let in_2 = SignedVariableInteger::<6>::new(53735);
        let in_3 = SignedVariableInteger::<2>::new(-3);

        in_1.ser(&mut writer);
        in_2.ser(&mut writer);
        in_3.ser(&mut writer);

        let buffer = writer.to_bytes();

        // Read
        let mut reader = BitReader::new(&buffer);

        let out_1 = Serde::de(&mut reader).unwrap();
        let out_2 = Serde::de(&mut reader).unwrap();
        let out_3 = Serde::de(&mut reader).unwrap();

        assert_eq!(in_1, out_1);
        assert_eq!(in_2, out_2);
        assert_eq!(in_3, out_3);
    }

    // Floats

    #[test]
    fn read_write_unsigned_float() {
        // Write
        let mut writer = BitWriter::new();

        let in_1 = UnsignedFloat::<7, 1>::new(12.3);
        let in_2 = UnsignedFloat::<20, 2>::new(5352.21);
        let in_3 = UnsignedFloat::<5, 1>::new(3.0);

        in_1.ser(&mut writer);
        in_2.ser(&mut writer);
        in_3.ser(&mut writer);

        let buffer = writer.to_bytes();

        // Read
        let mut reader = BitReader::new(&buffer);

        let out_1: UnsignedFloat<7, 1> = Serde::de(&mut reader).unwrap();
        let out_2: UnsignedFloat<20, 2> = Serde::de(&mut reader).unwrap();
        let out_3: UnsignedFloat<5, 1> = Serde::de(&mut reader).unwrap();

        assert!(
            (in_1.get() - out_1.get()).abs() < 0.0001,
            "{} != {}",
            in_1.get(),
            out_1.get()
        );
        assert!(
            (in_2.get() - out_2.get()).abs() < 0.0001,
            "{} != {}",
            in_2.get(),
            out_2.get()
        );
        assert!(
            (in_3.get() - out_3.get()).abs() < 0.0001,
            "{} != {}",
            in_3.get(),
            out_3.get()
        );
    }

    #[test]
    fn read_write_signed_float() {
        let mut writer = BitWriter::new();

        let in_1 = SignedFloat::<7, 1>::new(-12.3);
        let in_2 = SignedFloat::<20, 2>::new(5352.21);
        let in_3 = SignedFloat::<5, 1>::new(-3.0);

        in_1.ser(&mut writer);
        in_2.ser(&mut writer);
        in_3.ser(&mut writer);

        let buffer = writer.to_bytes();
        let mut reader = BitReader::new(&buffer);

        let out_1: SignedFloat<7, 1> = Serde::de(&mut reader).unwrap();
        let out_2: SignedFloat<20, 2> = Serde::de(&mut reader).unwrap();
        let out_3: SignedFloat<5, 1> = Serde::de(&mut reader).unwrap();

        assert!(
            (in_1.get() - out_1.get()).abs() < 0.0001,
            "{} != {}",
            in_1.get(),
            out_1.get()
        );
        assert!(
            (in_2.get() - out_2.get()).abs() < 0.0001,
            "{} != {}",
            in_2.get(),
            out_2.get()
        );
        assert!(
            (in_3.get() - out_3.get()).abs() < 0.0001,
            "{} != {}",
            in_3.get(),
            out_3.get()
        );
    }

    #[test]
    fn read_write_unsigned_variable_float() {
        let mut writer = BitWriter::new();

        let in_1 = UnsignedVariableFloat::<3, 1>::new(2.3);
        let in_2 = UnsignedVariableFloat::<5, 2>::new(153.22);
        let in_3 = UnsignedVariableFloat::<2, 1>::new(3.0);

        in_1.ser(&mut writer);
        in_2.ser(&mut writer);
        in_3.ser(&mut writer);

        let buffer = writer.to_bytes();
        let mut reader = BitReader::new(&buffer);

        let out_1: UnsignedVariableFloat<3, 1> = Serde::de(&mut reader).unwrap();
        let out_2: UnsignedVariableFloat<5, 2> = Serde::de(&mut reader).unwrap();
        let out_3: UnsignedVariableFloat<2, 1> = Serde::de(&mut reader).unwrap();

        assert!(
            (in_1.get() - out_1.get()).abs() < 0.0001,
            "{} != {}",
            in_1.get(),
            out_1.get()
        );
        assert!(
            (in_2.get() - out_2.get()).abs() < 0.0001,
            "{} != {}",
            in_2.get(),
            out_2.get()
        );
        assert!(
            (in_3.get() - out_3.get()).abs() < 0.0001,
            "{} != {}",
            in_3.get(),
            out_3.get()
        );
    }

    #[test]
    fn read_write_signed_variable_float() {
        let mut writer = BitWriter::new();

        let in_1 = SignedVariableFloat::<5, 1>::new(-66.8);
        let in_2 = SignedVariableFloat::<6, 2>::new(537.35);
        let in_3 = SignedVariableFloat::<2, 1>::new(-3.0);

        in_1.ser(&mut writer);
        in_2.ser(&mut writer);
        in_3.ser(&mut writer);

        let buffer = writer.to_bytes();
        let mut reader = BitReader::new(&buffer);

        let out_1: SignedVariableFloat<5, 1> = Serde::de(&mut reader).unwrap();
        let out_2: SignedVariableFloat<6, 2> = Serde::de(&mut reader).unwrap();
        let out_3: SignedVariableFloat<2, 1> = Serde::de(&mut reader).unwrap();

        assert!(
            (in_1.get() - out_1.get()).abs() < 0.0001,
            "{} != {}",
            in_1.get(),
            out_1.get()
        );
        assert!(
            (in_2.get() - out_2.get()).abs() < 0.0001,
            "{} != {}",
            in_2.get(),
            out_2.get()
        );
        assert!(
            (in_3.get() - out_3.get()).abs() < 0.0001,
            "{} != {}",
            in_3.get(),
            out_3.get()
        );
    }
}
