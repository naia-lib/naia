
use naia_shared::Manifest;

use super::protocol::{Protocol, Position, Name, Marker, StringMessage, Auth};

pub fn manifest_load() -> Manifest<Protocol> {
    let mut manifest = Manifest::<Protocol>::new();

    manifest.register_state(Auth::get_builder());
    manifest.register_state(StringMessage::get_builder());
    manifest.register_state(Position::get_builder());
    manifest.register_state(Name::get_builder());
    manifest.register_state(Marker::get_builder());

    manifest
}
