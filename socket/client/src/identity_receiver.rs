use naia_socket_shared::IdentityToken;

pub enum IdentityReceiverResult {
    Waiting,
    Success(IdentityToken),
    ErrorResponseCode(u16),
}

/// Used to receive an IdentityToken from the Client Socket
pub trait IdentityReceiver: IdentityReceiverClone + Send + Sync {
    /// Receives an IdentityToken from the Client Socket
    fn receive(&mut self) -> IdentityReceiverResult;
}

/// Used to clone Box<dyn IdentityReceiver>
pub trait IdentityReceiverClone {
    /// Clone the boxed IdentityReceiver
    fn clone_box(&self) -> Box<dyn IdentityReceiver>;
}

impl<T: 'static + IdentityReceiver + Clone> IdentityReceiverClone for T {
    fn clone_box(&self) -> Box<dyn IdentityReceiver> {
        Box::new(self.clone())
    }
}

impl Clone for Box<dyn IdentityReceiver> {
    fn clone(&self) -> Box<dyn IdentityReceiver> {
        IdentityReceiverClone::clone_box(self.as_ref())
    }
}
