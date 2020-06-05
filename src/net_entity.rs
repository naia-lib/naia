use crate::{EntityType};

pub trait NetEntity<T: EntityType> {
//    fn write_create(&self, out_bytes: &mut Vec<u8>);
//    fn write_update(&self, out_bytes: &mut Vec<u8>);
    fn read(&mut self, in_bytes: &[u8]);
    fn to_type(&self) -> T;
//    fn read_update(in_bytes: &mut [u8]) -> Self;
//    fn disappear(&self);
//    fn delete(&self);
}

//impl<Z: ManifestType, T: 'static + NetEntity<Z>> Copy for T {}