pub type PacketIndex = u16;
pub type Tick = u16;
pub type MessageIndex = u16;
pub type ShortMessageIndex = u8;
pub type GlobalRequestId = u64;
pub type GlobalResponseId = u64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HostType {
    Server,
    Client,
}