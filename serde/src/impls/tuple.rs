use crate::{
    reader_writer::{BitReader, BitWriter},
    error::SerdeErr,
    serde::Serde,
};
macro_rules! impl_reflect_tuple {
    {$($index:tt : $name:tt),*} => {
        impl<$($name : Serde,)*> Serde for ($($name,)*) {
            fn ser(&self, writer: &mut BitWriter) {
                $(writer.write(&self.$index);)*
            }
            fn de(reader: &mut BitReader) -> Result<($($name,)*), SerdeErr> {
                Ok(($(reader.read::<$name>()?, )*))
            }
        }
    }
}

impl_reflect_tuple! {0: A}
impl_reflect_tuple! {0: A, 1: B}
impl_reflect_tuple! {0: A, 1: B, 2: C}
impl_reflect_tuple! {0: A, 1: B, 2: C, 3: D}
impl_reflect_tuple! {0: A, 1: B, 2: C, 3: D, 4: E}
impl_reflect_tuple! {0: A, 1: B, 2: C, 3: D, 4: E, 5: F}
impl_reflect_tuple! {0: A, 1: B, 2: C, 3: D, 4: E, 5: F, 6: G}
impl_reflect_tuple! {0: A, 1: B, 2: C, 3: D, 4: E, 5: F, 6: G, 7: H}
impl_reflect_tuple! {0: A, 1: B, 2: C, 3: D, 4: E, 5: F, 6: G, 7: H, 8: I}
impl_reflect_tuple! {0: A, 1: B, 2: C, 3: D, 4: E, 5: F, 6: G, 7: H, 8: I, 9: J}
impl_reflect_tuple! {0: A, 1: B, 2: C, 3: D, 4: E, 5: F, 6: G, 7: H, 8: I, 9: J, 10: K}
impl_reflect_tuple! {0: A, 1: B, 2: C, 3: D, 4: E, 5: F, 6: G, 7: H, 8: I, 9: J, 10: K, 11: L}

// Tests

#[cfg(test)]
mod tests {
    use crate::{BitReader, BitWriter};

    #[test]
    fn read_write() {
        // Write
        let mut writer = BitWriter::new();

        let in_1 = (true, -7532, "Hello tuple!".to_string(), Some(5));
        #[allow(unused_parens)]
        let in_2 = (5);
        let in_3 = (true, false, true, None, 4815, "Tuples tuples..".to_string());
        let in_4 = (332, "Goodbye tuple...".to_string());

        writer.write(&in_1);
        writer.write(&in_2);
        writer.write(&in_3);
        writer.write(&in_4);

        let (buffer_length, buffer) = writer.flush();

        // Read

        let mut reader = BitReader::new(buffer_length, buffer);

        let out_1 = reader.read().unwrap();
        let out_2 = reader.read().unwrap();
        let out_3: (bool, bool, bool, Option<String>, u16, String) = reader.read().unwrap();
        let out_4 = reader.read().unwrap();

        assert_eq!(in_1, out_1);
        assert_eq!(in_2, out_2);
        assert_eq!(in_3, out_3);
        assert_eq!(in_4, out_4);
    }
}


