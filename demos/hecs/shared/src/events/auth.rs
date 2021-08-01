
use naia_derive::Event;
use naia_shared::{Event, Property};

use super::Events;

#[derive(Event, Clone)]
#[type_name = "Events"]
pub struct Auth {
    pub username: Property<String>,
    pub password: Property<String>,
}

impl Auth {
    fn is_guaranteed() -> bool {
        false
    }

    pub fn new(username: &str, password: &str) -> Auth {
        return Auth::new_complete(username.to_string(), password.to_string());
    }
}
