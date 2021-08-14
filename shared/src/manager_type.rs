/// Every data packet transmitted has data specific to either the Event or
/// Replicate managers. This value is written to differentiate those parts of the
/// payload.
#[derive(Copy, Clone, Debug, PartialEq)]
#[repr(u8)]
pub enum ManagerType {
    /// An EventManager
    Event = 1,
    /// An ReplicateManager
    Replicate = 2,
    /// An CommandManager
    Command = 3,
    /// Unknown Manager
    Unknown = 255,
}

impl From<u8> for ManagerType {
    fn from(orig: u8) -> Self {
        match orig {
            1 => return ManagerType::Event,
            2 => return ManagerType::Replicate,
            3 => return ManagerType::Command,
            _ => return ManagerType::Unknown,
        };
    }
}
