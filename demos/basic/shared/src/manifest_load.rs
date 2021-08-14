
use naia_shared::Manifest;

use super::protocol::{Protocol, Position, Name, Marker, StringMessage, Auth};

pub fn manifest_load() -> Manifest<Protocol> {
    let mut manifest = Manifest::<Protocol>::new();

    manifest.register_state(Auth::state_get_builder());
    manifest.register_state(StringMessage::state_get_builder());
    manifest.register_state(Position::state_get_builder());
    manifest.register_state(Name::state_get_builder());
    manifest.register_state(Marker::state_get_builder());

    manifest
}
