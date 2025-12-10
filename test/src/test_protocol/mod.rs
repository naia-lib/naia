/// Minimal test protocol for E2E testing
use bevy_ecs::prelude::Component;

use naia_shared::{Property, Protocol, Replicate, Message};

#[derive(Message, PartialEq, Eq)]
pub struct Auth {
    pub username: String,
    pub password: String,
}

impl Auth {
    pub fn new(username: &str, password: &str) -> Self {
        Self {
            username: username.to_string(),
            password: password.to_string(),
        }
    }
}

#[derive(Component, Replicate)]
pub struct Position {
    pub x: Property<f32>,
    pub y: Property<f32>,
}

impl Position {
    pub fn new(x: f32, y: f32) -> Self {
        Self::new_complete(x, y)
    }
}

pub fn protocol() -> Protocol {
    Protocol::builder()
        .add_component::<Position>()
        .add_message::<Auth>()
        .enable_client_authoritative_entities()
        .build()
}
