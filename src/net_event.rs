
use super::net_base::NetBase;

pub trait NetEvent: NetBase {
//    fn is_guaranteed() -> bool;
    fn write(&self, out_bytes: &mut Vec<u8>);
//    fn read(in_bytes: &mut [u8]) -> Self;
}