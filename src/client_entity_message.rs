use naia_shared::{LocalEntityKey};

#[derive(Clone)]
pub enum ClientEntityMessage {
    Create(LocalEntityKey),
    Update(LocalEntityKey),
    Delete(LocalEntityKey),
}