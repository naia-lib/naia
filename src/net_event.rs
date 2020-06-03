
use super::net_base::NetBase;

pub trait NetEvent<T>: NetBase<T> {
//    fn is_guaranteed() -> bool;
    fn write(&self, out_bytes: &mut Vec<u8>);
    fn read(&mut self, in_bytes: &[u8]);
}