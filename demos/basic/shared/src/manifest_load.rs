
use naia_shared::Manifest;

use super::components::{Components, Position, Name, Marker};
use super::events::{Events, StringMessage, Auth};

pub fn manifest_load() -> Manifest<Events, Components> {
    let mut manifest = Manifest::<Events, Components>::new();

    manifest.register_event(Auth::event_get_builder());
    manifest.register_event(StringMessage::event_get_builder());

    manifest.register_state(Position::state_get_builder());
    manifest.register_state(Name::state_get_builder());
    manifest.register_state(Marker::state_get_builder());

    manifest
}
