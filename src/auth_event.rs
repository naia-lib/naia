
use gaia_shared::{Event};
use crate::ExampleEvent;

#[derive(Clone)]
pub struct AuthEvent {
    username: Option<String>,
    password: Option<String>,
}

impl AuthEvent {
    pub fn init() -> AuthEvent {
        AuthEvent {
            username: None,
            password: None,
        }
    }

    pub fn new(username: String, password: String) -> Self {
        AuthEvent {
            username: Some(username),
            password: Some(password),
        }
    }

    pub fn get_username(&self) -> String {
        self.username.as_ref().unwrap().clone()
    }

    pub fn get_password(&self) -> String {
        self.password.as_ref().unwrap().clone()
    }
}

impl Event<ExampleEvent> for AuthEvent {
    fn is_guaranteed(&self) -> bool {
        false
    }

    fn to_type(&self) -> ExampleEvent {
        return ExampleEvent::AuthEvent(self.clone());
    }

    fn write(&self, buffer: &mut Vec<u8>) {
        let username_str = self.username.as_ref().unwrap();
        let password_str = self.password.as_ref().unwrap();
        let mut bytes = username_str.as_bytes().to_vec();
        buffer.push(bytes.len() as u8);
        buffer.append(&mut bytes);
        bytes = password_str.as_bytes().to_vec();
        buffer.append(&mut bytes);
    }

    fn read(&mut self, buffer: &[u8]) {
        let username_bytes_number: usize = (buffer[0] as usize) + 1;
        let username_bytes = &buffer[1..username_bytes_number];
        let password_bytes = &buffer[username_bytes_number..buffer.len()];
        let username_str = String::from_utf8_lossy(username_bytes).to_string();
        let password_str = String::from_utf8_lossy(password_bytes).to_string();
        self.username = Some(username_str);
        self.password = Some(password_str);
    }
}