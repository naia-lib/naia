
use naia_shared::Manifest;

use super::components::{Components, Position, Name, Marker};
use super::events::{Events, StringMessage, Auth};

pub fn manifest_load() -> Manifest<Events, Components> {
    let mut manifest = Manifest::<Events, Components>::new();

    manifest.register_event(Auth::get_builder());
    manifest.register_event(StringMessage::get_builder());

    manifest.register_state(Position::get_builder());
    manifest.register_state(Name::get_builder());
    manifest.register_state(Marker::get_builder());

    manifest
}