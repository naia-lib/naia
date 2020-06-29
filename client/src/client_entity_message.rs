use naia_shared::LocalEntityKey;

#[derive(Debug, Clone)]
pub enum ClientEntityMessage {
    Create(LocalEntityKey),
    Update(LocalEntityKey),
    Delete(LocalEntityKey),
}
